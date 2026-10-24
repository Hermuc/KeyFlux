//! 0x40C 查询桥 (T0'): 引擎把查询交给本进程执行，取代"spawn es.exe → 落盘 →
//! AHK 逐行解析 → 0x406 回推"的旧搬运链。
//!
//! 流程: WM_COPYDATA(dwData='KFC1') 进入 → [`super::ipc`] 查询 Everything →
//! 写共享内存镜像 (引擎读回 items, 供其打开/导航逻辑使用) → 本体渲染
//! (`AppEvent::SetResults`)。任何一步失败都返回 0 —— 引擎侧据此**自动回落**
//! es.exe 旧通道，行为与升级前完全一致 (旧命令框 exe 同样不认识 'KFC1',
//! 天然兼容混布)。
//!
//! 为什么用共享内存而不是回发消息: AHK 引擎线程在 `DllCall(SendMessage…)`
//! 期间不泵消息，若本进程同步 SendMessage 回推会死锁; `PostMessage` 又不能
//! 携带跨进程内存。引擎在 0x40C 调用**返回后**读本进程已写好的镜像段，
//! 时序天然同步 (单写单读，无需锁)。

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_ALL_ACCESS,
    MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
};

use super::ipc::{Client, Hit, SearchRequest};
use crate::results::Item;

/// 0x40C 载荷/消息魔数 `'K','F','C','1'` (小端 u32; dwData 同值)。
/// 魔数即版本号 —— 未来格式变更换 '2', 旧发送方自然被拒。
pub const QUERY_MAGIC: u32 = 0x3143_464B;

/// 镜像段魔数 `'K','F','M','1'`。
pub const MIRROR_MAGIC: u32 = 0x314D_464B;

/// 镜像段名 (`Local\` = 当前会话命名空间; 引擎与命令框同用户同会话)。
pub const MIRROR_NAME: &str = "Local\\KeyFlux-CommandInput-Search-v1";

/// 镜像段容量: 1000 条上限 × ~150B/条 的余量 (超限截断, 见 [`encode_mirror`])。
pub const MIRROR_CAP: usize = 256 * 1024;

/// 镜像条目标志位: 目录。
pub const MIRROR_FLAG_DIR: u8 = 0x01;

/// 0x40C 请求 (载荷解码产物)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryReq {
    pub max_results: u32,
    /// IPC3 排序属性 ID (0=名称; 引擎侧镜像 Everything GUI 的当前排序)。
    pub sort_property: u32,
    pub sort_descending: bool,
    pub text: String,
}

/// 0x40C 载荷解码。布局 (全小端):
/// ```text
///   [0..4)  魔数 'KFC1'
///   [4..8)  max_results u32
///   [8..12) sort_property u32
///   [12..16) sort_descending u32 (0/1)
///   [16..20) query_len u32
///   [20..)   query UTF-8
/// ```
/// 任何结构问题返回 `None` (调用方忽略该消息, 绝不让对端数据带崩命令框)。
pub fn decode_query(bytes: &[u8]) -> Option<QueryReq> {
    if bytes.len() < 20 || bytes.len() > 64 * 1024 {
        return None;
    }
    let magic = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
    if magic != QUERY_MAGIC {
        return None;
    }
    let max_results = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
    let sort_property = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
    let sort_descending = u32::from_le_bytes(bytes[12..16].try_into().ok()?) != 0;
    let qlen = u32::from_le_bytes(bytes[16..20].try_into().ok()?) as usize;
    if 20 + qlen != bytes.len() {
        return None;
    }
    let text = String::from_utf8(bytes[20..].to_vec()).ok()?;
    Some(QueryReq {
        max_results,
        sort_property,
        sort_descending,
        text,
    })
}

/// 镜像编码 (纯函数, 单测锁定格式; 引擎侧 AHK 镜像解析在 EverythingHost)。
/// 布局 (全小端):
/// ```text
///   [0..4) 魔数 'KFM1'
///   [4..8) count u32
///   重复 count 次: [flags u8 (bit0=目录)][nlen u32][name UTF-8][NUL][plen u32][path UTF-8][NUL]
/// ```
/// 字符串各带一个不计入长度的 NUL —— 引擎侧 AHK `StrGet(指针, "UTF-8")` 按 NUL 停,
/// 无长度参数可用 (这是两端格式的一部分, 不是可选项)。
/// 超出 [`MIRROR_CAP`] 时截断条目 (count 相应缩小) —— 条数策略本就由引擎的
/// max_results 控制, 截断只是防御异常参数。
pub fn encode_mirror(hits: &[Hit]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + 98 * hits.len());
    out.extend_from_slice(&MIRROR_MAGIC.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // count 占位, 收尾回填
    let mut count: u32 = 0;
    for hit in hits {
        let name = hit.name.as_bytes();
        let full = hit.full_path();
        let path = full.as_bytes();
        let need = 1 + 4 + name.len() + 1 + 4 + path.len() + 1;
        if out.len() + need > MIRROR_CAP {
            break;
        }
        let flags = if hit.is_dir { MIRROR_FLAG_DIR } else { 0 };
        out.push(flags);
        out.extend_from_slice(&(name.len() as u32).to_le_bytes());
        out.extend_from_slice(name);
        out.push(0);
        out.extend_from_slice(&(path.len() as u32).to_le_bytes());
        out.extend_from_slice(path);
        out.push(0);
        count += 1;
    }
    out[4..8].copy_from_slice(&count.to_le_bytes());
    out
}

/// 桥: 缓存 IPC 客户端 + 惰性创建镜像段 (单线程 UI 线程持有, 无锁)。
#[derive(Debug)]
pub struct Bridge {
    client: Option<Client>,
    map: HANDLE,
    view: *mut u8,
}

impl Bridge {
    pub fn new() -> Self {
        Self {
            client: None,
            map: HANDLE::default(),
            view: std::ptr::null_mut(),
        }
    }

    /// 执行一次查询。失败 (Everything 未运行/超时/断连) 返回 `None`,
    /// 调用方据此让引擎回落旧通道; 出错的连接按约定丢弃, 下次重连。
    pub fn query(&mut self, req: &QueryReq) -> Option<Vec<Hit>> {
        if self.client.is_none() {
            self.client = Some(Client::connect().ok()?);
        }
        let search = SearchRequest {
            text: req.text.clone(),
            offset: 0,
            max_results: req.max_results,
            match_path: false,
            sort_property: req.sort_property,
            sort_descending: req.sort_descending,
        };
        match self.client.as_ref()?.search(&search) {
            Ok(reply) => Some(reply.items),
            Err(_) => {
                self.client = None; // Err 后连接不可复用 (见 ipc/mod.rs 约定)
                None
            }
        }
    }

    /// 写镜像段 (惰性创建)。失败返回 `false` (段创建失败/写入超限)。
    pub fn write_mirror(&mut self, hits: &[Hit]) -> bool {
        if !self.ensure_mapping() {
            return false;
        }
        let bytes = encode_mirror(hits);
        if bytes.len() > MIRROR_CAP || bytes.is_empty() {
            return false;
        }
        // SAFETY: view 指向本进程创建的 MIRROR_CAP 字节提交内存; 单线程持有,
        // 此刻无其他读者 (引擎只在 0x40C 返回后读)。
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.view, bytes.len());
        }
        true
    }

    /// 惰性创建命名镜像段 (页文件支撑, 读写全开)。
    fn ensure_mapping(&mut self) -> bool {
        if !self.view.is_null() {
            return true;
        }
        let mut name: Vec<u16> = MIRROR_NAME.encode_utf16().collect();
        name.push(0);
        // SAFETY: name 以 NUL 结尾; INVALID_HANDLE_VALUE = 页文件支撑段; 大小 ≤ 4GB 低 32 位足够。
        let map = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                None,
                PAGE_READWRITE,
                0,
                MIRROR_CAP as u32,
                PCWSTR(name.as_ptr()),
            )
        }
        .ok();
        let Some(map) = map else {
            return false;
        };
        // SAFETY: map 为刚创建的有效段句柄; 全量映射 (失败 = Value 为空)。
        let view = unsafe { MapViewOfFile(map, FILE_MAP_ALL_ACCESS, 0, 0, MIRROR_CAP) };
        if view.Value.is_null() {
            // SAFETY: 创建成功但映射失败 —— 关闭句柄避免泄漏。
            unsafe {
                let _ = CloseHandle(map);
            }
            return false;
        }
        self.map = map;
        self.view = view.Value as *mut u8;
        true
    }
}

impl Default for Bridge {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        // SAFETY: 句柄与视图由本结构独占持有, Drop 各释放一次; 忽略失败 (进程回收兜底)。
        unsafe {
            if !self.view.is_null() {
                let _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.view as *mut core::ffi::c_void,
                });
            }
            if !self.map.is_invalid() {
                let _ = CloseHandle(self.map);
            }
        }
    }
}

/// 把 IPC 命中转为本体结果行 (Flow Launcher 双行版式: 标题=文件名, 副标题=完整路径)。
pub fn hits_to_items(hits: &[Hit]) -> Vec<Item> {
    hits.iter()
        .map(|h| Item::new(h.name.clone(), h.full_path()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(name: &str, path: &str, is_dir: bool) -> Hit {
        Hit {
            name: name.into(),
            path: path.into(),
            is_dir,
        }
    }

    #[test]
    fn decode_query_golden() {
        let mut b = Vec::new();
        b.extend_from_slice(&QUERY_MAGIC.to_le_bytes());
        b.extend_from_slice(&300u32.to_le_bytes());
        b.extend_from_slice(&5u32.to_le_bytes()); // date modified
        b.extend_from_slice(&1u32.to_le_bytes()); // descending
        b.extend_from_slice(&2u32.to_le_bytes());
        b.extend_from_slice(b"ab");
        let req = decode_query(&b).expect("decode");
        assert_eq!(
            req,
            QueryReq {
                max_results: 300,
                sort_property: 5,
                sort_descending: true,
                text: "ab".into(),
            }
        );
    }

    #[test]
    fn decode_query_rejects_malformed() {
        assert!(decode_query(&[]).is_none());
        assert!(decode_query(&[0u8; 8]).is_none());
        let mut b = Vec::new();
        b.extend_from_slice(&QUERY_MAGIC.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes()); // len=0 但总长 > 20
        b.extend_from_slice(&[0u8; 5]);
        assert!(decode_query(&b).is_none());
    }

    #[test]
    fn mirror_roundtrip() {
        let hits = vec![
            hit("src", r"D:\Code\src", true),
            hit("a.txt", "C:\\", false),
        ];
        let m = encode_mirror(&hits);
        assert_eq!(&m[0..4], &MIRROR_MAGIC.to_le_bytes());
        assert_eq!(u32::from_le_bytes(m[4..8].try_into().unwrap()), 2);
        // 逐条回读 (模拟引擎侧解析: 字符串按 len 读 + 跳过 NUL 终止符)
        let mut off = 8usize;
        for (i, h) in hits.iter().enumerate() {
            let flags = m[off];
            off += 1;
            let nlen = u32::from_le_bytes(m[off..off + 4].try_into().unwrap()) as usize;
            off += 4;
            let name = &m[off..off + nlen];
            assert_eq!(m[off + nlen], 0, "item {i} name NUL 终止");
            off += nlen + 1;
            let plen = u32::from_le_bytes(m[off..off + 4].try_into().unwrap()) as usize;
            off += 4;
            let path = &m[off..off + plen];
            assert_eq!(m[off + plen], 0, "item {i} path NUL 终止");
            off += plen + 1;
            assert_eq!(name, h.name.as_bytes(), "item {i}");
            assert_eq!(path, h.full_path().as_bytes(), "item {i}");
            assert_eq!(flags & MIRROR_FLAG_DIR != 0, h.is_dir, "item {i}");
        }
        assert_eq!(off, m.len(), "无残留字节");
    }

    #[test]
    fn mirror_empty() {
        let m = encode_mirror(&[]);
        assert_eq!(m.len(), 8);
        assert_eq!(u32::from_le_bytes(m[4..8].try_into().unwrap()), 0);
    }

    #[test]
    fn mirror_truncates_at_cap() {
        // 单条超大 (名字 ~240KB) 的命中会触发容量截断: 至少保留头与 count 自洽。
        let big = "x".repeat(MIRROR_CAP);
        let m = encode_mirror(&[hit(&big, "C:\\", false), hit("ok", "C:\\", false)]);
        let count = u32::from_le_bytes(m[4..8].try_into().unwrap());
        assert_eq!(count, 0, "首条即超限 ⇒ 全部截断");
        assert!(m.len() <= MIRROR_CAP);
    }
}
