//! SEARCH 命令（code 7）的请求构造与结果流解析。
//!
//! 格式来源：voidtools/ES `src/es.c`（SEARCH 包构造）与 `src/ipc3.c`
//! （ipc3_result_list_init 的响应头/条目读取），均按 64 位 SIZE_T 口径
//! （请求带 IPC3_SEARCH_FLAG_64BIT，响应 valid_flags 回显同位）。

use super::pipe::Client;
use super::Error;

/// 命令码（ES ipc3.h IPC3_COMMAND_*）。
pub(crate) const COMMAND_SEARCH: u32 = 7;
const COMMAND_IS_DB_LOADED: u32 = 8;

/// 搜索标志（ES ipc3.h IPC3_SEARCH_FLAG_*，节选）。
const FLAG_MATCH_PATH: u32 = 0x0000_0004;
const FLAG_TOTAL_SIZE: u32 = 0x0000_0800;
const FLAG_64BIT: u32 = 0x0000_4000;
/// 排序标志（ES ipc3.h IPC3_SEARCH_SORT_FLAG_DESCENDING）。
const SORT_FLAG_DESCENDING: u32 = 0x0000_0001;

/// 属性 ID（ES Everything3.h EVERYTHING3_PROPERTY_ID_*）。
const PROPERTY_NAME: u32 = 0;
const PROPERTY_PATH: u32 = 1;

/// 属性值类型（ES ipc3.h IPC3_PROPERTY_VALUE_TYPE_*，此处仅列需要分派的族）。
const VALUE_BYTE: u8 = 1;
const VALUE_WORD: u8 = 2;
const VALUE_DWORD: u8 = 3;
const VALUE_Q1K: u8 = 4;
const VALUE_UINT64: u8 = 5;
const VALUE_SIZE_T: u8 = 11;
const VALUE_Q1K_I32: u8 = 12;
const VALUE_Q1M_I32: u8 = 13;
const VALUE_PSTRING: u8 = 8;
const VALUE_PSTRING_MULTI: u8 = 9;
const VALUE_PSTRING_REF: u8 = 10;
const VALUE_PSTRING_FOLDER_REF: u8 = 14;
const VALUE_PSTRING_FILE_REF: u8 = 15;
const VALUE_BYTE_TEXT: u8 = 20;
const VALUE_WORD_TEXT: u8 = 18;

/// 属性请求标志 FORMAT|HIGHLIGHT：值恒为（VLQ+UTF-8）字符串。
const PREQUEST_FORMATTED: u32 = 0x0000_0003;

/// 条目标志（ES ipc3.h IPC3_RESULT_LIST_ITEM_FLAG_*）。
const ITEM_FLAG_FOLDER: u8 = 0x01;

/// 结果条目数分配上限（防御异常响应；正常请求 max_results 远小于此）。
const MAX_ITEMS: u64 = 100_000;

/// 字节流读取抽象：生产实现为管道 [`ReplyReader`]，测试实现为内存缓冲。
pub(crate) trait StreamRead {
    /// 从流中读满 `out`。
    fn read_exact(&mut self, out: &mut [u8]) -> Result<(), Error>;

    /// 读取一个 u32（小端）。
    fn read_u32(&mut self) -> Result<u32, Error> {
        let mut b = [0u8; 4];
        self.read_exact(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }

    /// 读取一个 u64（小端）。
    fn read_u64(&mut self) -> Result<u64, Error> {
        let mut b = [0u8; 8];
        self.read_exact(&mut b)?;
        Ok(u64::from_le_bytes(b))
    }

    /// 读取一个字节。
    fn read_u8(&mut self) -> Result<u8, Error> {
        let mut b = [0u8; 1];
        self.read_exact(&mut b)?;
        Ok(b[0])
    }

    /// 读取 VLQ 长度（编码梯度：byte<0xff；+word<0xffff；+dword<0xffffffff；+u64）。
    fn read_vlq(&mut self) -> Result<usize, Error> {
        let mut value = self.read_u8()? as usize;
        if value < 0xff {
            return Ok(value);
        }
        let mut pair = [0u8; 2];
        self.read_exact(&mut pair)?;
        let word = u16::from_le_bytes(pair) as usize;
        value += word;
        if word < 0xffff {
            return Ok(value);
        }
        let mut quad = [0u8; 4];
        self.read_exact(&mut quad)?;
        let dword = u32::from_le_bytes(quad) as usize;
        value += dword;
        if dword < 0xffff_ffff {
            return Ok(value);
        }
        let full = self.read_u64()?;
        value.checked_add(full as usize).ok_or(Error::BadResponse)
    }

    /// 读取 `len` 字节。
    fn read_bytes(&mut self, len: usize) -> Result<Vec<u8>, Error> {
        let mut buf = vec![0u8; len];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }
}

/// 一次搜索请求。
#[derive(Debug, Clone)]
pub struct SearchRequest {
    /// 搜索文本（Everything 语法原样透传）。
    pub text: String,
    /// 视口偏移（分页起点）。
    pub offset: u64,
    /// 视口条数（返回结果上限）。
    pub max_results: u32,
    /// 是否连路径一起匹配（IPC3 SEARCH_FLAG_MATCH_PATH）。
    pub match_path: bool,
    /// IPC3 排序属性 ID（0=名称；引擎侧镜像 Everything GUI 当前排序）。
    pub sort_property: u32,
    /// 是否降序。
    pub sort_descending: bool,
}

impl SearchRequest {
    /// 构造默认请求（偏移 0、不匹配路径、名称升序）。
    pub fn new(text: impl Into<String>, max_results: u32) -> SearchRequest {
        SearchRequest {
            text: text.into(),
            offset: 0,
            max_results,
            match_path: false,
            sort_property: PROPERTY_NAME,
            sort_descending: false,
        }
    }
}

/// 一条命中。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// 文件名。
    pub name: String,
    /// 父目录路径（Everything PATH 属性：不带尾反斜杠；卷根形如 `C:`）。
    pub path: String,
    /// 是否文件夹。
    pub is_dir: bool,
}

impl Hit {
    /// 完整路径（父目录 + 反斜杠 + 文件名）。
    pub fn full_path(&self) -> String {
        format!("{}\\{}", self.path, self.name)
    }
}

/// 搜索结果：总命中数 + 视口条目。
#[derive(Debug, Clone)]
pub struct SearchReply {
    /// 命中总数（文件夹 + 文件）。
    pub total: u64,
    /// 视口内条目（顺序 = 服务器排序，默认按名称升序）。
    pub items: Vec<Hit>,
}

impl Client {
    /// Everything 索引库是否已加载完毕（IS_DB_LOADED，code 8）。
    pub fn is_db_loaded(&self) -> Result<bool, Error> {
        Ok(self.simple_u32(COMMAND_IS_DB_LOADED)? != 0)
    }

    /// 执行一次搜索。
    pub fn search(&self, req: &SearchRequest) -> Result<SearchReply, Error> {
        let packet = build_request(req);
        let mut reader = self.search_stream(&packet)?;
        parse_reply(&mut reader)
    }
}

/// 构造 SEARCH 请求包（64 位 SIZE_T 口径）。
fn build_request(req: &SearchRequest) -> Vec<u8> {
    let text = req.text.as_bytes();
    let mut flags = FLAG_64BIT;
    if req.match_path {
        flags |= FLAG_MATCH_PATH;
    }

    let mut packet = Vec::with_capacity(16 + text.len() + 8 + 10 + 18);
    packet.extend_from_slice(&flags.to_le_bytes());
    write_vlq(&mut packet, text.len());
    packet.extend_from_slice(text);
    packet.extend_from_slice(&req.offset.to_le_bytes());
    packet.extend_from_slice(&u64::from(req.max_results).to_le_bytes());
    // 排序：按请求属性与方向（引擎侧镜像 Everything GUI 当前排序）。
    write_vlq(&mut packet, 1);
    packet.extend_from_slice(&req.sort_property.to_le_bytes());
    packet.extend_from_slice(
        &(if req.sort_descending {
            SORT_FLAG_DESCENDING
        } else {
            0
        })
        .to_le_bytes(),
    );
    // 属性请求：NAME 与 PATH 的原始值（flags=0，不带 FORMAT/HIGHLIGHT）。
    write_vlq(&mut packet, 2);
    packet.extend_from_slice(&PROPERTY_NAME.to_le_bytes());
    packet.extend_from_slice(&0u32.to_le_bytes());
    packet.extend_from_slice(&PROPERTY_PATH.to_le_bytes());
    packet.extend_from_slice(&0u32.to_le_bytes());
    packet
}

/// 写入 VLQ 长度（编码梯度与 ES ipc3_copy_len_vlq 一致）。
fn write_vlq(out: &mut Vec<u8>, value: usize) {
    if value < 0xff {
        out.push(value as u8);
        return;
    }
    out.push(0xff);
    let mut rest = value - 0xff;
    if rest < 0xffff {
        out.extend_from_slice(&(rest as u16).to_le_bytes());
        return;
    }
    out.extend_from_slice(&0xffffu16.to_le_bytes());
    rest -= 0xffff;
    if rest <= 0xffff_fffe {
        out.extend_from_slice(&(rest as u32).to_le_bytes());
        return;
    }
    out.extend_from_slice(&0xffff_ffffu32.to_le_bytes());
    rest -= 0xffff_ffff;
    out.extend_from_slice(&(rest as u64).to_le_bytes());
}

/// 解析 SEARCH 结果流（ipc3_result_list_init 口径）。
fn parse_reply<R: StreamRead + ?Sized>(reader: &mut R) -> Result<SearchReply, Error> {
    // 响应头。
    let valid_flags = reader.read_u32()?;
    let is64 = valid_flags & FLAG_64BIT != 0;
    let folder_count = read_sizet(reader, is64)?;
    let file_count = read_sizet(reader, is64)?;
    if valid_flags & FLAG_TOTAL_SIZE != 0 {
        reader.read_u64()?; // 未请求，但按回显标志跳过以稳妥。
    }
    let _viewport_offset = read_sizet(reader, is64)?;
    let viewport_count = read_sizet(reader, is64)?;

    // 排序回显：跳过。
    let sort_count = reader.read_vlq()?;
    for _ in 0..sort_count {
        reader.read_u32()?;
        reader.read_u32()?;
    }

    // 属性请求回显：服务器告知每个属性的实际值类型。
    let preq_count = reader.read_vlq()?;
    let mut preqs = Vec::with_capacity(preq_count.min(64));
    for _ in 0..preq_count {
        let id = reader.read_u32()?;
        let flags = reader.read_u32()?;
        let value_type = reader.read_u8()?;
        preqs.push((id, flags, value_type));
    }

    // 条目流。
    if viewport_count > MAX_ITEMS {
        return Err(Error::BadResponse);
    }
    let total = folder_count
        .checked_add(file_count)
        .ok_or(Error::BadResponse)?;
    let mut items = Vec::with_capacity(viewport_count as usize);
    for _ in 0..viewport_count {
        let item_flags = reader.read_u8()?;
        let mut hit = Hit {
            name: String::new(),
            path: String::new(),
            is_dir: item_flags & ITEM_FLAG_FOLDER != 0,
        };
        for (id, flags, value_type) in &preqs {
            let value = read_value(reader, *flags, *value_type, is64)?;
            match *id {
                PROPERTY_NAME => hit.name = value,
                PROPERTY_PATH => hit.path = value,
                _ => {} // 未来扩展属性：读出但暂不使用。
            }
        }
        items.push(hit);
    }
    Ok(SearchReply { total, items })
}

/// SIZE_T 读取（宽度由响应回显的 64BIT 位决定）。
fn read_sizet<R: StreamRead + ?Sized>(reader: &mut R, is64: bool) -> Result<u64, Error> {
    if is64 {
        reader.read_u64()
    } else {
        Ok(u64::from(reader.read_u32()?))
    }
}

/// 按值类型读取一个属性值：字符串值返回内容，其余类型读出并丢弃。
fn read_value<R: StreamRead + ?Sized>(
    reader: &mut R,
    flags: u32,
    value_type: u8,
    is64: bool,
) -> Result<String, Error> {
    if flags & PREQUEST_FORMATTED != 0 {
        return read_pstring(reader);
    }
    match value_type {
        VALUE_PSTRING
        | VALUE_PSTRING_MULTI
        | VALUE_PSTRING_REF
        | VALUE_PSTRING_FOLDER_REF
        | VALUE_PSTRING_FILE_REF => read_pstring(reader),
        VALUE_BYTE | VALUE_BYTE_TEXT => {
            reader.read_u8()?;
            Ok(String::new())
        }
        VALUE_WORD | VALUE_WORD_TEXT => {
            reader.read_u32().map(|_| ())?;
            Ok(String::new())
        }
        VALUE_DWORD | VALUE_Q1K | VALUE_Q1K_I32 | VALUE_Q1M_I32 => {
            reader.read_u32()?;
            Ok(String::new())
        }
        VALUE_UINT64 => {
            reader.read_u64()?;
            Ok(String::new())
        }
        VALUE_SIZE_T => {
            read_sizet(reader, is64)?;
            Ok(String::new())
        }
        _ => Err(Error::BadResponse),
    }
}

/// PSTRING 族：VLQ 长度 + UTF-8 字节。
fn read_pstring<R: StreamRead + ?Sized>(reader: &mut R) -> Result<String, Error> {
    let len = reader.read_vlq()?;
    let bytes = reader.read_bytes(len)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用带帧读取器：把裸数据包成单条 200 消息，复现 ReplyReader 的帧语义。
    struct FramedCursor {
        data: Vec<u8>,
        pos: usize,
        avail: usize,
        eof: bool,
    }

    impl FramedCursor {
        fn single_message(raw: &[u8]) -> FramedCursor {
            let mut framed = 200u32.to_le_bytes().to_vec();
            framed.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            framed.extend_from_slice(raw);
            FramedCursor {
                data: framed,
                pos: 0,
                avail: 0,
                eof: false,
            }
        }
    }

    impl StreamRead for FramedCursor {
        fn read_exact(&mut self, out: &mut [u8]) -> Result<(), Error> {
            let mut filled = 0;
            while filled < out.len() {
                if self.avail == 0 {
                    if self.eof || self.pos + 8 > self.data.len() {
                        return Err(Error::Disconnected);
                    }
                    let code =
                        u32::from_le_bytes(self.data[self.pos..self.pos + 4].try_into().unwrap());
                    let size = u32::from_le_bytes(
                        self.data[self.pos + 4..self.pos + 8].try_into().unwrap(),
                    ) as usize;
                    self.pos += 8;
                    match code {
                        200 => {
                            self.avail = size;
                            self.eof = true;
                        }
                        100 => self.avail = size,
                        400..=599 => return Err(Error::Server(code)),
                        _ => return Err(Error::BadResponse),
                    }
                }
                let n = self
                    .avail
                    .min(out.len() - filled)
                    .min(self.data.len() - self.pos);
                out[filled..filled + n].copy_from_slice(&self.data[self.pos..self.pos + n]);
                self.pos += n;
                self.avail -= n;
                filled += n;
            }
            Ok(())
        }
    }

    #[test]
    fn vlq_roundtrip_boundaries() {
        // 覆盖每个编码梯度的边界（含跨梯度进位点）。
        for value in [
            0usize,
            1,
            0xfe,
            0xff,
            0x100,
            0xff + 0xfffe,
            0xff + 0xffff,
            0xff + 0xffff + 1,
            0xff + 0xffff + 0xffff_ffff,
        ] {
            let mut out = Vec::new();
            write_vlq(&mut out, value);
            let mut reader = FramedCursor::single_message(&out);
            let read = reader.read_vlq().expect("vlq read");
            assert_eq!(read, value, "value {value:#x} bytes {out:02x?}");
        }
    }

    #[test]
    fn request_packet_golden() {
        let req = SearchRequest {
            text: "ab".into(),
            offset: 1,
            max_results: 12,
            match_path: true,
            sort_property: 5, // date modified
            sort_descending: true,
        };
        let packet = build_request(&req);
        let parts: [&[u8]; 12] = [
            &0x0000_4004u32.to_le_bytes()[..], // flags = 64BIT | MATCH_PATH
            &[0x02, b'a', b'b'],               // VLQ(2) + "ab"
            &1u64.to_le_bytes()[..],           // offset
            &12u64.to_le_bytes()[..],          // max_results
            &[0x01],                           // 排序数 = 1
            &5u32.to_le_bytes()[..],           //   按修改时间
            &1u32.to_le_bytes()[..],           //   降序
            &[0x02],                           // 属性请求数 = 2
            &0u32.to_le_bytes()[..],
            &0u32.to_le_bytes()[..], // NAME, raw
            &1u32.to_le_bytes()[..],
            &0u32.to_le_bytes()[..], // PATH, raw
        ];
        let expected: Vec<u8> = parts.concat();
        assert_eq!(packet, expected);
    }

    #[test]
    fn reply_parse_synthetic() {
        let mut body = Vec::new();
        body.extend_from_slice(&FLAG_64BIT.to_le_bytes()); // valid_flags
        body.extend_from_slice(&1u64.to_le_bytes()); // folder_count
        body.extend_from_slice(&2u64.to_le_bytes()); // file_count
        body.extend_from_slice(&0u64.to_le_bytes()); // viewport_offset
        body.extend_from_slice(&2u64.to_le_bytes()); // viewport_count
        body.push(0x01); // 排序数 = 1
        body.extend_from_slice(&0u32.to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes());
        body.push(0x02); // 属性请求数 = 2
        body.extend_from_slice(&PROPERTY_NAME.to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes());
        body.push(VALUE_PSTRING_REF);
        body.extend_from_slice(&PROPERTY_PATH.to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes());
        body.push(VALUE_PSTRING_FOLDER_REF);
        // 条目 1：文件夹 keyflux @ D:\PortableApps
        body.push(ITEM_FLAG_FOLDER);
        body.push(7);
        body.extend_from_slice(b"keyflux");
        body.push(15);
        body.extend_from_slice(b"D:\\PortableApps");
        // 条目 2：文件 a.txt @ C:
        body.push(0x00);
        body.push(5);
        body.extend_from_slice(b"a.txt");
        body.push(2);
        body.extend_from_slice(b"C:");

        let mut reader = FramedCursor::single_message(&body);
        let reply = parse_reply(&mut reader).expect("parse");
        assert_eq!(reply.total, 3);
        assert_eq!(reply.items.len(), 2);
        assert!(reply.items[0].is_dir);
        assert_eq!(reply.items[0].name, "keyflux");
        assert_eq!(reply.items[0].path, "D:\\PortableApps");
        assert_eq!(reply.items[0].full_path(), "D:\\PortableApps\\keyflux");
        assert!(!reply.items[1].is_dir);
        assert_eq!(reply.items[1].full_path(), "C:\\a.txt");
    }

    #[test]
    #[ignore = "需要运行中的 Everything 1.5（命名管道）；探测不到时静默跳过"]
    fn live_search_against_running_everything() {
        let client = match Client::connect() {
            Ok(client) => client,
            Err(Error::NotRunning) => return,
            Err(e) => panic!("connect: {e}"),
        };
        assert!(client.is_db_loaded().expect("is_db_loaded"));

        let reply = client
            .search(&SearchRequest::new("KeyFlux", 12))
            .expect("search");
        assert!(reply.total > 0, "total = {}", reply.total);
        assert!(!reply.items.is_empty());
        assert!(
            reply
                .items
                .iter()
                .any(|hit| hit.name.to_ascii_lowercase().contains("keyflux")),
            "hits: {:?}",
            reply
                .items
                .iter()
                .map(Hit::full_path)
                .take(4)
                .collect::<Vec<_>>()
        );

        // 宽查询：单字符取满 12 条（活体基线见 memory §4.7：总命中 70 万+）。
        let wide = client
            .search(&SearchRequest::new("a", 12))
            .expect("wide search");
        assert_eq!(wide.items.len(), 12);
    }
}
