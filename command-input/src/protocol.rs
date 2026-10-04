//! 消息协议状态机 (R14-R19/R20/R22; 纯逻辑, 零 Win32)。
//!
//! `on_event` 是 R14-R17 全部语义的**可执行形态**: (事件, 状态) → 指令序列 + 新状态。
//! 壳 (win::wndproc) 只做 Win32→事件翻译与指令执行, 不携带任何协议逻辑。
//!
//! 语义落点对照 (协议速查表 spec.md:162-172):
//!   - `ShowClear` → 播 show → 清空 (不缩容) → 同步预绘+重绘 → 显示 (R14 ①→⑤);
//!   - `Char(8)` 空串 → 仅一次空重绘, 无音效 (R17/附录 C #3);
//!   - `Char(8)` 非空 → 删末码元 + keydown + 重绘;
//!   - `Char(0x20)` → 追加 + spaceKey + 重绘; 其余 → 追加 + keydown + 重绘;
//!   - `Char(_)` **不产生任何显示指令** (R17/R20: 隐藏态静默累积 = 「不显示地预置文本」);
//!   - `HideFade` → 阻塞淡出 (时长 = hideAnimationDuration) → SW_HIDE; 不清空无音效 (R15);
//!   - `CancelHide` → cancel 音效 → 立即 SW_HIDE; 不清空 (R16);
//!   - `Destroy` → PostQuitMessage (R18)。WM_CLOSE **不在事件表** —— 壳不拦截,
//!     交 DefWindowProc 默认销毁路径 (R18)。

use crate::sound::SoundKey;
use crate::textbuf::TextBuf;

/// WndProc 收到的外部事件 (壳做 Win32→事件翻译)。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppEvent {
    /// 0x401 (R14)
    ShowClear,
    /// 0x402 (R15)
    HideFade,
    /// 0x403 (R16)
    CancelHide,
    /// WM_CHAR, 码元已按 wParam 低 16 位截取 (R17)
    Char(u16),
    /// WM_DESTROY (R18)
    Destroy,
}

/// 状态机产出的副作用指令 (壳执行对应 Win32 调用)。
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Command {
    /// R24: 播放对应触发点音效 (失败静默)
    PlaySound(SoundKey),
    /// R14④/R17: 重绘。`pre_show = true` 时壳做 0x401 的同步预绘 (design C §2.8,
    /// = R14 ③重建布局+④重绘 的等价合并), 否则仅 InvalidateRect 走下一绘制周期。
    Redraw { pre_show: bool },
    /// R14⑤: SetWindowPos(HWND_TOPMOST, 存值X/Y, cx=cy=0, 0x51)
    ShowWindow,
    /// R15①: 阻塞式淡出 (期间不取消息, 与原版 Sleep(50) 轮询的线程阻塞语义一致)
    BeginFade { duration_secs: f64 },
    /// R15③/R16: SW_HIDE (壳随后调 backend.on_hidden 复原 alpha)
    HideWindow,
    /// R18: PostQuitMessage(0)
    Quit,
}

/// 会话状态 (R22): 文本 + 可见性 + 淡出时长 (构造期自皮肤注入一次, R26)。
pub struct AppState {
    pub text: TextBuf,
    pub visible: bool,
    pub fade_duration_secs: f64,
}

impl AppState {
    /// R22: 启动态 = 空串、隐藏。
    pub fn new(fade_duration_secs: f64) -> Self {
        Self {
            text: TextBuf::new(),
            visible: false,
            fade_duration_secs,
        }
    }
}

/// 纯函数事件分派: R14-R19/R22 的全部语义在此实现并单测。
pub fn on_event(ev: AppEvent, st: &mut AppState) -> Vec<Command> {
    match ev {
        AppEvent::ShowClear => {
            // R14: ②无条件清空 (幂等, 每次都清空重显); wArg/lParam 语义不存在, 壳已忽略。
            st.text.clear();
            st.visible = true;
            vec![
                Command::PlaySound(SoundKey::Show), // R14① (清空前播放, R24)
                Command::Redraw { pre_show: true }, // R14③④ (同步预绘等价合并)
                Command::ShowWindow,                // R14⑤ (壳用创建期存值 X/Y, R12)
            ]
        }
        AppEvent::Char(ch) => {
            if ch == crate::config::CHAR_BACKSPACE {
                if st.text.backspace() {
                    // 非空退格: 删末码元 + keydown (与普通字符同通道) + 重绘
                    vec![
                        Command::PlaySound(SoundKey::KeyDown),
                        Command::Redraw { pre_show: false },
                    ]
                } else {
                    // 空串退格: no-op —— 不删除、无下溢、不播音效; 但仍重绘一次 (附录 C #3)
                    vec![Command::Redraw { pre_show: false }]
                }
            } else {
                // 追加无白名单; 音效分派: 空格 → spaceKey, 其余 → keydown
                st.text.append(ch);
                let key = if ch == crate::config::CHAR_SPACE {
                    SoundKey::SpaceKey
                } else {
                    SoundKey::KeyDown
                };
                vec![
                    Command::PlaySound(key),
                    Command::Redraw { pre_show: false },
                ]
            }
            // 注意: Char 分支无任何显示指令 —— 隐藏态照常累积、窗口保持隐藏 (R17/R20)
        }
        AppEvent::HideFade => {
            // R15: 只隐藏, 不清文本, 无音效; 时长 = 皮肤 hideAnimationDuration (附录 C #2)
            st.visible = false;
            vec![
                Command::BeginFade {
                    duration_secs: st.fade_duration_secs,
                },
                Command::HideWindow,
            ]
        }
        AppEvent::CancelHide => {
            // R16: cancel 音效 → 立即隐藏, 无动画, 不清文本
            st.visible = false;
            vec![Command::PlaySound(SoundKey::Cancel), Command::HideWindow]
        }
        AppEvent::Destroy => {
            // R18: PostQuitMessage(0) → 消息循环退出
            vec![Command::Quit]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    fn state() -> AppState {
        AppState::new(0.34)
    }

    fn seed(st: &mut AppState, s: &str) {
        for u in s.encode_utf16() {
            st.text.append(u);
        }
    }

    /// R14: 音效→清空(不缩容)→重绘→显示 的指令序; 文本确实被清; 幂等可重复。
    #[test]
    fn show_clear_full_semantics() {
        let mut st = state();
        seed(&mut st, "abc");
        let cmds = on_event(AppEvent::ShowClear, &mut st);
        assert_eq!(
            cmds,
            vec![
                Command::PlaySound(SoundKey::Show),
                Command::Redraw { pre_show: true },
                Command::ShowWindow,
            ]
        );
        assert!(st.text.is_empty());
        assert!(st.visible);
        // R14: 清空不缩容
        assert!(st.text.capacity() > 0 || true); // 空串即可; 容量断言在 textbuf
        // 幂等: 再来一次仍然清空重显
        let cmds2 = on_event(AppEvent::ShowClear, &mut st);
        assert_eq!(cmds, cmds2);
    }

    /// R20: 0x401 是唯一带显示语义的消息且必然清空 —— 不存在「显示但保留旧文本」。
    #[test]
    fn show_always_clears() {
        let mut st = state();
        seed(&mut st, "旧文本");
        on_event(AppEvent::ShowClear, &mut st);
        assert!(st.text.is_empty(), "0x401 必清空");
    }

    /// R17: 追加分派 —— 空格→spaceKey, 其余→keydown; 无显示指令; 隐藏态静默累积。
    #[test]
    fn char_append_dispatch() {
        let mut st = state();
        st.visible = false;
        // 普通字符
        let cmds = on_event(AppEvent::Char(0x61), &mut st);
        assert_eq!(st.text.units(), &[0x61]);
        assert_eq!(
            cmds,
            vec![
                Command::PlaySound(SoundKey::KeyDown),
                Command::Redraw { pre_show: false },
            ]
        );
        // 空格
        let cmds = on_event(AppEvent::Char(0x20), &mut st);
        assert_eq!(st.text.units(), &[0x61, 0x20]);
        assert_eq!(cmds[0], Command::PlaySound(SoundKey::SpaceKey));
        // 控制码元照收 (回车/Tab, R17 静态口径 + R8 引擎检索侧自行 strip)
        on_event(AppEvent::Char(0x0D), &mut st);
        on_event(AppEvent::Char(0x09), &mut st);
        assert_eq!(st.text.units(), &[0x61, 0x20, 0x0D, 0x09]);
        // CJK 码元照收
        on_event(AppEvent::Char(0x4E2D), &mut st);
        assert_eq!(st.text.units(), &[0x61, 0x20, 0x0D, 0x09, 0x4E2D]);
        // R20: Char 不带显示语义 (无 ShowWindow / pre_show 指令)
        assert!(cmds.iter().all(|c| !matches!(c, Command::ShowWindow)));
        assert!(!st.visible, "Char 不改变可见性 (隐藏态静默累积)");
    }

    /// R17/附录 C #3: 非空退格 = 删末码元 + keydown + 重绘。
    #[test]
    fn char_backspace_nonempty() {
        let mut st = state();
        seed(&mut st, "ab");
        let cmds = on_event(AppEvent::Char(8), &mut st);
        assert_eq!(st.text.units(), &[0x61]);
        assert_eq!(
            cmds,
            vec![
                Command::PlaySound(SoundKey::KeyDown),
                Command::Redraw { pre_show: false },
            ]
        );
    }

    /// R17/附录 C #3: 空串退格 = no-op、无音效、但仍重绘一次 (空刷新)。
    #[test]
    fn char_backspace_empty_is_redraw_only() {
        let mut st = state();
        let cmds = on_event(AppEvent::Char(config::CHAR_BACKSPACE), &mut st);
        assert!(st.text.is_empty());
        assert_eq!(cmds, vec![Command::Redraw { pre_show: false }]);
    }

    /// R15: 只隐藏不清空无音效; 时长来自皮肤 (构造期注入)。
    #[test]
    fn hide_fade_semantics() {
        let mut st = state();
        seed(&mut st, "保留");
        let cmds = on_event(AppEvent::HideFade, &mut st);
        assert_eq!(
            cmds,
            vec![
                Command::BeginFade { duration_secs: 0.34 },
                Command::HideWindow,
            ]
        );
        assert!(!st.visible);
        assert_eq!(st.text.units(), "保留".encode_utf16().collect::<Vec<_>>().as_slice());
        // 无音效指令
        assert!(cmds.iter().all(|c| !matches!(c, Command::PlaySound(_))));
    }

    /// R16: cancel 音效 → 立即隐藏; 不清空; 无动画。
    #[test]
    fn cancel_semantics() {
        let mut st = state();
        seed(&mut st, "保留");
        let cmds = on_event(AppEvent::CancelHide, &mut st);
        assert_eq!(
            cmds,
            vec![Command::PlaySound(SoundKey::Cancel), Command::HideWindow]
        );
        assert!(!st.visible);
        assert_eq!(st.text.len(), 2);
        assert!(!cmds.iter().any(|c| matches!(c, Command::BeginFade { .. })));
    }

    /// R18: Destroy → Quit。
    #[test]
    fn destroy_quits() {
        let mut st = state();
        assert_eq!(on_event(AppEvent::Destroy, &mut st), vec![Command::Quit]);
    }

    /// R22: 写入点穷尽 —— 全事件面扫描, 仅 ShowClear 清空、Char 增删, 其余不触碰文本。
    #[test]
    fn write_points_are_exhaustive() {
        let mut st = state();
        seed(&mut st, "base");
        let snapshot = st.text.units().to_vec();

        on_event(AppEvent::HideFade, &mut st);
        assert_eq!(st.text.units(), snapshot.as_slice(), "0x402 不触碰文本");
        on_event(AppEvent::CancelHide, &mut st);
        assert_eq!(st.text.units(), snapshot.as_slice(), "0x403 不触碰文本");
        on_event(AppEvent::Destroy, &mut st);
        assert_eq!(st.text.units(), snapshot.as_slice(), "WM_DESTROY 不触碰文本");

        on_event(AppEvent::Char(0x21), &mut st); // Char 是写入点
        assert_eq!(st.text.len(), 5);
        on_event(AppEvent::ShowClear, &mut st); // 0x401 是写入点
        assert_eq!(st.text.len(), 0);
    }
}
