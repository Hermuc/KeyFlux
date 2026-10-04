//! 文本状态机 (R17/R22; 纯逻辑): 单行 UTF-16 码元序列。
//!
//! 语义对照 (spec.md R17/R22):
//!   - 追加无白名单、无长度上限 —— 空格 0x20 / 回车 0x0D / Tab / 代理对半码元照单全收;
//!   - 退格删末码元; 空串退格 no-op (返回 false, 协议层据此不播音效);
//!   - 清空**不缩容** (R14: 原版 assign(L"") 后容量与堆缓冲保留; Vec::clear 恰好同语义);
//!   - 启动态 = 空串 (R22)。

/// 单行 Unicode 文本 (UTF-16 码元存储, 与 WM_CHAR 通道同粒度)。
#[derive(Clone, Default)]
pub struct TextBuf {
    units: Vec<u16>,
}

impl TextBuf {
    /// R22: 启动 / 窗口创建完成时为空串。
    pub const fn new() -> Self {
        Self { units: Vec::new() }
    }

    /// R17 (ch != 8): 原样追加一个码元, 无白名单、无上限、扩容后内容逐字保留。
    pub fn append(&mut self, unit: u16) {
        self.units.push(unit);
    }

    /// R17 (ch == 8): 删末码元; `true` = 删了 (播 keydown), `false` = 空串 no-op (不播)。
    pub fn backspace(&mut self) -> bool {
        self.units.pop().is_some()
    }

    /// R14: 无条件清空, 容量保留不缩 (`Vec::clear` 保留 capacity)。
    pub fn clear(&mut self) {
        self.units.clear();
    }

    /// 当前码元序列 (渲染 / FrameState 消费)。
    pub fn units(&self) -> &[u16] {
        &self.units
    }

    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// R33 (读回通道预留): size 语义 = 码元数, 不含 NUL。
    pub fn len(&self) -> usize {
        self.units.len()
    }

    /// R14/R33: 容量观察 (清空不缩容的断言面)。
    pub fn capacity(&self) -> usize {
        self.units.capacity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R22: 初始空串。
    #[test]
    fn starts_empty() {
        let t = TextBuf::new();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
        assert_eq!(t.units(), &[] as &[u16]);
    }

    /// R17: 追加无白名单 —— 控制码元与代理对半码元照收, 顺序保留。
    #[test]
    fn append_without_whitelist() {
        let mut t = TextBuf::new();
        for ch in [0x61, 0x20, 0x0D, 0x09, 0x4E2D, 0xD83D, 0xDE00] {
            t.append(ch);
        }
        assert_eq!(t.units(), &[0x61, 0x20, 0x0D, 0x09, 0x4E2D, 0xD83D, 0xDE00]);
    }

    /// R17: 非空退格删末码元; 空串退格 no-op 返回 false。
    #[test]
    fn backspace_semantics() {
        let mut t = TextBuf::new();
        assert!(!t.backspace()); // 空串退格: no-op 无下溢
        t.append(0x61);
        t.append(0x62);
        assert!(t.backspace());
        assert_eq!(t.units(), &[0x61]);
        assert!(t.backspace());
        assert!(t.is_empty());
        assert!(!t.backspace());
    }

    /// R14/R22: 清空不缩容 —— 容量保留 (读回方不得以 size==0 推断 SSO 的行为基面)。
    #[test]
    fn clear_keeps_capacity() {
        let mut t = TextBuf::new();
        for i in 0..32u16 {
            t.append(0x61 + i);
        }
        let cap_before = t.capacity();
        assert!(cap_before >= 32);
        t.clear();
        assert_eq!(t.len(), 0);
        assert_eq!(t.capacity(), cap_before, "clear 必须保留容量 (R14 不缩容)");
        // 清空后可继续追加, 内容逐字保留
        t.append(0x4E2D);
        assert_eq!(t.units(), &[0x4E2D]);
    }

    /// R17: 超长文本持续增长 (无上限)。
    #[test]
    fn grows_without_limit() {
        let mut t = TextBuf::new();
        for _ in 0..10_000 {
            t.append(0x61);
        }
        assert_eq!(t.len(), 10_000);
    }
}
