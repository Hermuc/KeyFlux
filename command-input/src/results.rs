//! 结果列表面板 (命令框本体的向下延伸; 纯逻辑, 零 Win32, 可脱离窗口 cargo test)。
//!
//! 为什么在命令框里而不是另一个窗口 (2026-10-04 用户需求):
//!   历史形态由插件 (AHK) 自建一个 `Gui + ListView` 浮层挂在命令框下方, 再用
//!   `SetWindowRgn` 的「耳朵」裁切去拼轮廓假装连体 —— 视觉上仍是**两个独立组件**
//!   (接缝、圆角、阴影各自为政)。用户要求「列表必须是命令框自身的向下延伸」,
//!   参照 Flow Launcher / uTools 的单窗长高形态。
//!   故: 命令框窗口在结果出现时**自身向下长高**, 同一个圆角白框内
//!   [查询区 | 分隔线 | 结果行] 一体绘制; 列表状态 (项集/高亮/滚动) 由本模块持有,
//!   插件只经消息把数据推进来 (协议见 `config::APP_RESULTS_DATA` 等)。
//!
//! 职责边界: 本模块只做**数据与窗口**的推导 (哪些行可见 / 滚动到哪 / 载荷编解码),
//!   像素尺寸由 `geometry` 推、绘制由 `win::backend_gdi` 做。

/// 结果行展示数据 (2026-10-04 二版: Flow Launcher 双行版式)。
///
/// * `title`    = 文件名 (含后缀) —— 上行, 主文字色 (黑);
/// * `subtitle` = 完整路径 —— 下行, 灰 (小一号); 同时是**图标提取键**
///   (命令框按路径经 `SHGetFileInfoW` 取系统文件图标, 与 Flow Launcher 的
///   「UI 层自提图标」同架构; 空 = 无图标无路径, 单行居中, 供提示行)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub title: String,
    pub subtitle: String,
}

impl Item {
    pub fn new(title: impl Into<String>, subtitle: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
        }
    }

    /// 单行提示 (无路径无图标, 行内垂直居中)。
    pub fn hint(text: impl Into<String>) -> Self {
        Self::new(text, "")
    }
}

/// 0x406 载荷魔数: 字节序 = `K` `F` `R` `2` (小端 u32 → 0x3252_464B)。
///
/// 二版 (双行 + 路径图标键): 与一版 `KFR1` (单字符串/项) **不兼容** —— 魔数即版本号,
/// 旧框收到 KFR2 一律拒收 (静默忽略), 新框收到 KFR1 同样拒收; 两端同批部署。
pub const PAYLOAD_MAGIC: u32 = 0x3252_464B;

/// 载荷头字节数 = 魔数(4) + selected(4) + count(4)。
pub const HEADER_BYTES: usize = 12;

/// selected 字段的「无高亮」哨兵 (提示行: 有内容但不可选)。
pub const NO_SELECTION: i32 = -1;

/// 载荷字节数上限 (防御: 恶意/错误发送方不得让命令框按 cbData 申请巨量内存)。
pub const MAX_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;

/// 结果列表状态 (会话级; 与文本缓冲同生命周期, R22 的扩展)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultsState {
    items: Vec<Item>,
    /// 0 基高亮下标; `NO_SELECTION` = 无高亮 (提示行)
    selected: i32,
    /// 可视窗口起始行 (0 基); 由 `select`/`set_visible_rows` 自动收敛
    scroll: usize,
    /// 可视行数上限 (创建期按屏幕高度定案, 见 `geometry::max_list_rows`)
    visible_max: usize,
}

impl Default for ResultsState {
    fn default() -> Self {
        Self::new(0)
    }
}

impl ResultsState {
    pub fn new(visible_max: usize) -> Self {
        Self {
            items: Vec::new(),
            selected: NO_SELECTION,
            scroll: 0,
            visible_max,
        }
    }

    /// 可视行数上限 (创建期定案)。
    pub fn visible_max(&self) -> usize {
        self.visible_max
    }

    /// 重设可视行数上限 (WM_CREATE 拿到屏幕尺寸后调用一次); 会重新收敛滚动。
    pub fn set_visible_max(&mut self, n: usize) {
        self.visible_max = n.max(1);
        self.clamp_scroll();
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn item(&self, index: usize) -> Option<&Item> {
        self.items.get(index)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 0 基高亮下标; `NO_SELECTION` = 无。
    pub fn selected(&self) -> i32 {
        self.selected
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// 实际可见行数 = min(项数, 上限)。
    pub fn visible_rows(&self) -> usize {
        self.items.len().min(self.visible_max)
    }

    /// 可视行的 `[start, end)` 半开区间 (行号即 items 下标)。
    pub fn window(&self) -> (usize, usize) {
        let end = (self.scroll + self.visible_rows()).min(self.items.len());
        (self.scroll, end)
    }

    /// 整表替换 (0x406)。高亮越界时收敛; `-1` 保留为「无高亮」。
    pub fn set(&mut self, items: Vec<Item>, selected: i32) {
        self.items = items;
        self.scroll = 0;
        self.selected = NO_SELECTION; // select() 会按新项集收敛
        self.select(selected);
    }

    /// 移动高亮 (0x407)。@returns 是否发生了变化 (未变化时调用方无需重绘)。
    pub fn select(&mut self, selected: i32) -> bool {
        let n = self.items.len() as i32;
        let clamped = if n == 0 {
            NO_SELECTION
        } else {
            selected.clamp(NO_SELECTION, n - 1)
        };
        let changed = clamped != self.selected;
        self.selected = clamped;
        let scrolled = self.clamp_scroll();
        changed || scrolled
    }

    /// 收起 (0x408 / 0x401 / 隐藏): 清空项集与状态。@returns 是否原本非空。
    pub fn clear(&mut self) -> bool {
        let had = !self.items.is_empty() || self.scroll != 0;
        self.items.clear();
        self.selected = NO_SELECTION;
        self.scroll = 0;
        had
    }

    /// 滚动收敛: 高亮行必需落在可视窗口内; 无高亮时保持在有效范围。
    /// @returns 是否移动了滚动窗口。
    fn clamp_scroll(&mut self) -> bool {
        let before = self.scroll;
        let vis = self.visible_rows();
        if vis == 0 || self.items.len() <= vis {
            self.scroll = 0;
            return before != self.scroll;
        }
        if self.selected >= 0 {
            let s = self.selected as usize;
            if s < self.scroll {
                self.scroll = s;
            } else if s >= self.scroll + vis {
                self.scroll = s + 1 - vis;
            }
        }
        let max_scroll = self.items.len() - vis;
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
        before != self.scroll
    }
}

/// 0x406 载荷编码 (引擎侧 AHK 的 `EverythingHost.BuildResultsPayload` 是本函数的镜像;
/// 两端**逐字节**必须一致 —— 单测锁定格式)。
///
/// 布局 (全小端):
/// ```text
///   [0..4)   魔数 'KFR2'
///   [4..8)   selected  i32  (-1 = 无高亮; 0 基)
///   [8..12)  count     u32
///   重复 count 次: [title_len u32][title UTF-8][sub_len u32][sub UTF-8]
/// ```
pub fn encode_payload(items: &[Item], selected: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_BYTES + 48 * items.len());
    out.extend_from_slice(&PAYLOAD_MAGIC.to_le_bytes());
    out.extend_from_slice(&selected.to_le_bytes());
    out.extend_from_slice(&(items.len() as u32).to_le_bytes());
    for it in items {
        let t = it.title.as_bytes();
        let s = it.subtitle.as_bytes();
        out.extend_from_slice(&(t.len() as u32).to_le_bytes());
        out.extend_from_slice(t);
        out.extend_from_slice(&(s.len() as u32).to_le_bytes());
        out.extend_from_slice(s);
    }
    out
}

/// 0x406 载荷解码。任何结构问题 (魔数不符 / 截断 / 超限) 一律 `None` ——
/// 调用方忽略该消息, 绝不让对端数据把命令框带崩。
pub fn decode_payload(bytes: &[u8]) -> Option<(Vec<Item>, i32)> {
    if bytes.len() < HEADER_BYTES || bytes.len() > MAX_PAYLOAD_BYTES {
        return None;
    }
    let magic = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
    if magic != PAYLOAD_MAGIC {
        return None;
    }
    let selected = i32::from_le_bytes(bytes[4..8].try_into().ok()?);
    let count = u32::from_le_bytes(bytes[8..12].try_into().ok()?) as usize;
    let mut off = HEADER_BYTES;
    let mut items: Vec<Item> = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        // title
        if off + 4 > bytes.len() {
            return None;
        }
        let tlen = u32::from_le_bytes(bytes[off..off + 4].try_into().ok()?) as usize;
        off += 4;
        if off + tlen > bytes.len() {
            return None;
        }
        let title = String::from_utf8_lossy(&bytes[off..off + tlen]).into_owned();
        off += tlen;
        // subtitle
        if off + 4 > bytes.len() {
            return None;
        }
        let slen = u32::from_le_bytes(bytes[off..off + 4].try_into().ok()?) as usize;
        off += 4;
        if off + slen > bytes.len() {
            return None;
        }
        let subtitle = String::from_utf8_lossy(&bytes[off..off + slen]).into_owned();
        off += slen;
        items.push(Item { title, subtitle });
    }
    Some((items, selected))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(n: usize) -> Vec<Item> {
        (0..n)
            .map(|i| Item::new(format!("file{i}.txt"), format!("C:\\dir\\file{i}.txt")))
            .collect()
    }

    /// 载荷往返: 项集/高亮逐字节一致 (AHK 侧镜像的格式锁定)。
    #[test]
    fn payload_round_trip() {
        let src = vec![
            Item::new("a", "C:\\a.txt"),
            Item::new("路径.txt", "C:\\中文\\路径.txt"),
            Item::hint(""),
        ];
        let bytes = encode_payload(&src, 1);
        assert_eq!(&bytes[0..4], b"KFR2");
        assert_eq!(
            bytes.len(),
            HEADER_BYTES
                + (4 + 1 + 4 + "C:\\a.txt".len())
                + (4 + "路径.txt".len() + 4 + "C:\\中文\\路径.txt".len())
                + (4 + 0 + 4 + 0)
        );
        let (items, sel) = decode_payload(&bytes).expect("decode");
        assert_eq!(items, src);
        assert_eq!(sel, 1);
    }

    /// 空表 + 无高亮: 合法载荷 (收起指令仍走 0x408, 但空表必须可解)。
    #[test]
    fn payload_empty_and_no_selection() {
        let bytes = encode_payload(&[], NO_SELECTION);
        assert_eq!(bytes.len(), HEADER_BYTES);
        let (items, sel) = decode_payload(&bytes).expect("decode");
        assert!(items.is_empty());
        assert_eq!(sel, NO_SELECTION);
    }

    /// 解码防御: 魔数不符 (含旧版 KFR1 —— 魔数即版本号) / 截断 / 超限 一律 None。
    #[test]
    fn decode_rejects_malformed() {
        assert!(decode_payload(&[]).is_none());
        assert!(decode_payload(b"XXXX").is_none());
        let mut bad = encode_payload(&[Item::hint("abc")], 0);
        bad[0] = b'X';
        assert!(decode_payload(&bad).is_none());
        // 旧版一版载荷 (单字符串布局) 必须被拒 —— KFR1 魔数不符
        let mut old = encode_payload(&[Item::hint("abc")], 0);
        old[3] = b'1';
        assert!(decode_payload(&old).is_none());
        let good = encode_payload(&[Item::hint("abc")], 0);
        assert!(decode_payload(&good[..good.len() - 1]).is_none()); // 截断
        assert!(decode_payload(&good[..8]).is_none()); // 头不全
        let huge = vec![0u8; MAX_PAYLOAD_BYTES + 1];
        assert!(decode_payload(&huge).is_none());
    }

    /// 解码: count 声明大于实际数据 ⇒ None (不做部分接受)。
    #[test]
    fn decode_rejects_count_overflow() {
        let mut b = encode_payload(&[Item::hint("a")], 0);
        b[8..12].copy_from_slice(&9u32.to_le_bytes());
        assert!(decode_payload(&b).is_none());
    }

    /// set: 高亮越界收敛到末行; 空表 → 无高亮。
    #[test]
    fn set_clamps_selection() {
        let mut r = ResultsState::new(12);
        r.set(items(3), 99);
        assert_eq!(r.selected(), 2);
        r.set(items(3), -1);
        assert_eq!(r.selected(), NO_SELECTION);
        r.set(Vec::new(), 0);
        assert_eq!(r.selected(), NO_SELECTION);
        assert!(r.is_empty());
    }

    /// 可视窗口 + 滚动跟随高亮 (列表比可视区长时)。
    #[test]
    fn scroll_follows_selection() {
        let mut r = ResultsState::new(3);
        r.set(items(10), 0);
        assert_eq!(r.visible_rows(), 3);
        assert_eq!(r.window(), (0, 3));
        r.select(2); // 仍在第一屏
        assert_eq!(r.scroll(), 0);
        r.select(3); // 越界 → 下滚一行
        assert_eq!(r.scroll(), 1);
        assert_eq!(r.window(), (1, 4));
        r.select(9); // 末行 → 滚动到最后一屏
        assert_eq!(r.scroll(), 7);
        assert_eq!(r.window(), (7, 10));
        r.select(0); // 回到首行 → 回到顶
        assert_eq!(r.scroll(), 0);
    }

    /// 项数少于可视行数: 永远不滚动。
    #[test]
    fn no_scroll_when_fits() {
        let mut r = ResultsState::new(12);
        r.set(items(4), 3);
        assert_eq!(r.visible_rows(), 4);
        assert_eq!(r.window(), (0, 4));
        r.select(0);
        assert_eq!(r.scroll(), 0);
    }

    /// select 返回「是否发生变化」: 同值不重绘; 变化含滚动。
    #[test]
    fn select_reports_change() {
        let mut r = ResultsState::new(2);
        r.set(items(6), 0);
        assert!(!r.select(0), "同值不应报告变化");
        assert!(r.select(1), "索引变化应报告变化");
        assert!(r.select(5), "仅滚动也应报告变化");
        assert!(!r.select(5));
    }

    /// 可视上限收缩后滚动收敛 (屏幕变矮的兜底路径)。
    #[test]
    fn shrink_visible_max_reclamps_scroll() {
        let mut r = ResultsState::new(6);
        r.set(items(10), 9);
        assert_eq!(r.window(), (4, 10));
        r.set_visible_max(3);
        assert_eq!(r.window(), (7, 10));
    }

    /// clear 报告「原本是否非空」(决定是否值得重排窗口)。
    #[test]
    fn clear_reports_dirty() {
        let mut r = ResultsState::new(4);
        assert!(!r.clear());
        r.set(items(2), 0);
        assert!(r.clear());
        assert!(r.is_empty());
        assert_eq!(r.selected(), NO_SELECTION);
        assert!(!r.clear());
    }
}
