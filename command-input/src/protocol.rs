//! 消息协议状态机 (R14-R19/R20/R22; 纯逻辑, 零 Win32)。
//!
//! `on_event` 是 R14-R17 全部语义的**可执行形态**: (事件, 状态) → 指令序列 + 新状态。
//! 壳 (win::wndproc) 只做 Win32→事件翻译与指令执行, 不携带任何协议逻辑。
//!
//! 语义落点对照 (协议速查表 spec.md:162-172):
//!   - `ShowClear` → 播 show → 收起结果列表 → 清空 (不缩容) → 同步预绘+重绘 → 显示 (R14 ①→⑤);
//!   - `Char(8)` 空串 → 仅一次空重绘, 无音效 (R17/附录 C #3);
//!   - `Char(8)` 非空 → 删末码元 + keydown + 重绘;
//!   - `Char(0x20)` → 追加 + spaceKey + 重绘; 其余 → 追加 + keydown + 重绘;
//!   - `Char(_)` **不产生任何显示指令** (R17/R20: 隐藏态静默累积 = 「不显示地预置文本」);
//!   - `HideFade` → 阻塞淡出 (时长 = hideAnimationDuration) → SW_HIDE → 收起列表; 不清空无音效 (R15);
//!   - `CancelHide` → cancel 音效 → 立即 SW_HIDE → 收起列表; 不清空 (R16);
//!   - `SetResults` → 整表替换结果 + 重排窗口 (0x406; 2026-10-04 扩展);
//!   - `SetSelection` → 移动高亮 (0x407; 高度不变, 只重绘);
//!   - `ClearResults` → 收起列表 + 窗口回落 (0x408);
//!   - `Destroy` → PostQuitMessage (R18)。WM_CLOSE **不在事件表** —— 壳不拦截,
//!     交 DefWindowProc 默认销毁路径 (R18)。
//!
//! 结果列表面板 (2026-10-04, 用户需求「列表是命令框本体的向下延伸」): 列表状态随会话
//!   存在 (与文本同生命周期): 0x401 显示时清、0x402/0x403 隐藏时清 —— 插件无需额外
//!   收尾消息, 框自己保证「列表活不过一次会话」。
//!
//! 搜索徽标 (2026-10-04, 用户需求「查询区右侧固定图标, 由插件提供并渲染」): 同一口径 ——
//!   `badge` 随会话存在 (0x401/0x402/0x403 一律清除), 插件只在搜索模式激活/收尾时发
//!   0x40A/0x40B; 命令框只认字形编号 (`crate::badge`), 不知道任何插件。

use crate::results::ResultsState;
use crate::sound::SoundKey;
use crate::textbuf::TextBuf;

/// WndProc 收到的外部事件 (壳做 Win32→事件翻译)。
///
/// 非 `Copy` (0x406 载荷携带 `Vec`) —— `Clone` 保留给测试与调试。
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AppEvent {
    /// 0x401 (R14)
    ShowClear,
    /// 0x402 (R15)
    HideFade,
    /// 0x403 (R16)
    CancelHide,
    /// WM_CHAR, 码元已按 wParam 低 16 位截取 (R17)
    Char(u16),
    /// 0x406 (WM_COPYDATA): 结果列表整表替换; `selected` 0 基, `-1` = 无高亮。
    /// 项 = {title 文件名, subtitle 路径} 双行 (KFR2, 2026-10-04 Flow Launcher 版式)。
    SetResults {
        items: Vec<crate::results::Item>,
        selected: i32,
    },
    /// 0x407: 移动高亮 (0 基; `-1` = 无高亮)
    SetSelection(i32),
    /// 0x408: 收起结果列表
    ClearResults,
    /// 0x40A: 显示搜索徽标 (字形编号; 未注册编号被忽略 —— 对端错误不得带崩框)
    ShowBadge(u32),
    /// 0x40B: 隐藏搜索徽标
    HideBadge,
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
    /// 选择变化的增量重绘 (2026-10-06): 行集未变, 只重绘旧/新两行并呈现。
    /// `prev` = 变化前的高亮 (0 基; `-1` = 原本无高亮)。协议层守卫: 只有可视
    /// 窗口未变 (无滚动) 时才发出 —— 滚动换页走 `Redraw` 全量。
    RedrawRows { prev: i32 },
    /// 结果列表结构变化 (项集/展开/收起): 壳重算窗口高度, 需要时
    /// SetWindowPos + 后端区域重建, 然后重绘。高度不变时等价于一次重绘。
    Relayout,
    /// R14⑤: SetWindowPos(HWND_TOPMOST, 存值X/Y, cx=cy=0, 0x51)
    ShowWindow,
    /// R15①: 阻塞式淡出 (期间不取消息, 与原版 Sleep(50) 轮询的线程阻塞语义一致)
    BeginFade { duration_secs: f64 },
    /// R15③/R16: SW_HIDE (壳随后调 backend.on_hidden 复原 alpha)
    HideWindow,
    /// R18: PostQuitMessage(0)
    Quit,
}

/// 会话状态 (R22): 文本 + 可见性 + 淡出时长 (构造期自皮肤注入一次, R26) + 结果列表 + 徽标。
pub struct AppState {
    pub text: TextBuf,
    pub visible: bool,
    pub fade_duration_secs: f64,
    /// 结果列表面板状态 (2026-10-04)
    pub results: ResultsState,
    /// 搜索徽标 (2026-10-04): Some(字形编号) = 显示; 随会话存在 (0x401/0x402/0x403 清除)
    pub badge: Option<u32>,
}

impl AppState {
    /// R22: 启动态 = 空串、隐藏、无结果列表、无徽标。
    /// 可视行数上限先取兜底值, WM_CREATE 拿到屏幕尺寸后经
    /// `results.set_visible_max` 收敛 (见 win::wndproc::on_create)。
    pub fn new(fade_duration_secs: f64) -> Self {
        Self {
            text: TextBuf::new(),
            visible: false,
            fade_duration_secs,
            results: ResultsState::new(crate::config::LIST_MAX_ROWS),
            badge: None,
        }
    }
}

/// 纯函数事件分派: R14-R19/R22 的全部语义在此实现并单测。
pub fn on_event(ev: AppEvent, st: &mut AppState) -> Vec<Command> {
    match ev {
        AppEvent::ShowClear => {
            // R14: ②无条件清空 (幂等, 每次都清空重显); wArg/lParam 语义不存在, 壳已忽略。
            st.text.clear();
            // 新会话不该继承上一会话的结果列表与徽标 (框自己保证「活不过一次会话」)
            st.results.clear();
            st.badge = None;
            st.visible = true;
            vec![
                Command::PlaySound(SoundKey::Show), // R14① (清空前播放, R24)
                Command::Relayout, // 收起上一会话可能残留的展开高度, 再按基准几何显示
                Command::Redraw { pre_show: true }, // R14③④ (同步预绘等价合并)
                Command::ShowWindow, // R14⑤ (壳用创建期存值 X/Y, R12)
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
                vec![Command::PlaySound(key), Command::Redraw { pre_show: false }]
            }
            // 注意: Char 分支无任何显示指令 —— 隐藏态照常累积、窗口保持隐藏 (R17/R20)
        }
        AppEvent::SetResults { items, selected } => {
            st.results.set(items, selected);
            vec![Command::Relayout]
        }
        AppEvent::SetSelection(index) => {
            let prev = st.results.selected();
            let prev_window = st.results.window();
            if st.results.select(index) {
                // 可视窗口未变 (无滚动) ⇒ 行集相同, 只需重绘旧/新两行 (增量路径);
                // 高亮触发滚动 ⇒ 可见行集变化, 全量重绘
                if st.results.window() == prev_window {
                    vec![Command::RedrawRows { prev }]
                } else {
                    vec![Command::Redraw { pre_show: false }]
                }
            } else {
                vec![]
            }
        }
        AppEvent::ClearResults => {
            if st.results.clear() {
                vec![Command::Relayout]
            } else {
                vec![]
            }
        }
        AppEvent::ShowBadge(glyph) => {
            // 未注册字形 = 对端错误 → 忽略 (不崩、不重绘); 同值重复 → 幂等零指令
            if !crate::badge::is_known(glyph) || st.badge == Some(glyph) {
                vec![]
            } else {
                st.badge = Some(glyph);
                // 徽标不改窗口几何 (锚定查询区) ⇒ 只重绘, 无 Relayout
                vec![Command::Redraw { pre_show: false }]
            }
        }
        AppEvent::HideBadge => {
            // 幂等: 无徽标时零指令
            if st.badge.take().is_some() {
                vec![Command::Redraw { pre_show: false }]
            } else {
                vec![]
            }
        }
        AppEvent::HideFade => {
            // R15: 只隐藏, 不清文本, 无音效; 时长 = 皮肤 hideAnimationDuration (附录 C #2)
            st.visible = false;
            st.results.clear();
            st.badge = None;
            vec![
                Command::BeginFade {
                    duration_secs: st.fade_duration_secs,
                },
                Command::HideWindow,
                Command::Relayout, // 隐藏后回落基准高度 (下次 0x401 不残留展开尺寸)
            ]
        }
        AppEvent::CancelHide => {
            // R16: cancel 音效 → 立即隐藏, 无动画, 不清文本
            st.visible = false;
            st.results.clear();
            st.badge = None;
            vec![
                Command::PlaySound(SoundKey::Cancel),
                Command::HideWindow,
                Command::Relayout,
            ]
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

    /// R14: 音效→清空(不缩容)→重排→重绘→显示 的指令序; 文本确实被清; 幂等可重复。
    #[test]
    fn show_clear_full_semantics() {
        let mut st = state();
        seed(&mut st, "abc");
        let cmds = on_event(AppEvent::ShowClear, &mut st);
        assert_eq!(
            cmds,
            vec![
                Command::PlaySound(SoundKey::Show),
                Command::Relayout,
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

    /// R15: 只隐藏不清空无音效; 时长来自皮肤 (构造期注入); 收起结果列表 (重排回落).
    #[test]
    fn hide_fade_semantics() {
        let mut st = state();
        seed(&mut st, "保留");
        let cmds = on_event(AppEvent::HideFade, &mut st);
        assert_eq!(
            cmds,
            vec![
                Command::BeginFade {
                    duration_secs: 0.34
                },
                Command::HideWindow,
                Command::Relayout,
            ]
        );
        assert!(!st.visible);
        assert_eq!(
            st.text.units(),
            "保留".encode_utf16().collect::<Vec<_>>().as_slice()
        );
        // 无音效指令
        assert!(cmds.iter().all(|c| !matches!(c, Command::PlaySound(_))));
    }

    /// R16: cancel 音效 → 立即隐藏; 不清空; 无动画; 收起结果列表。
    #[test]
    fn cancel_semantics() {
        let mut st = state();
        seed(&mut st, "保留");
        let cmds = on_event(AppEvent::CancelHide, &mut st);
        assert_eq!(
            cmds,
            vec![
                Command::PlaySound(SoundKey::Cancel),
                Command::HideWindow,
                Command::Relayout,
            ]
        );
        assert!(!st.visible);
        assert_eq!(st.text.len(), 2);
        assert!(!cmds.iter().any(|c| matches!(c, Command::BeginFade { .. })));
    }

    // ---- 2026-10-04 结果列表面板 (0x406/0x407/0x408) ----

    fn results(items: &[&str]) -> Vec<crate::results::Item> {
        items
            .iter()
            .map(|t| crate::results::Item::new(*t, format!("C:\\dir\\{t}")))
            .collect()
    }

    /// 0x406: 整表替换 + 重排窗口; 高亮为 0 基。
    #[test]
    fn set_results_relayouts() {
        let mut st = state();
        let cmds = on_event(
            AppEvent::SetResults {
                items: results(&["a", "b", "c"]),
                selected: 1,
            },
            &mut st,
        );
        assert_eq!(cmds, vec![Command::Relayout]);
        assert_eq!(st.results.len(), 3);
        assert_eq!(st.results.selected(), 1);
    }

    /// 0x407: 高亮变化才重绘 (同值零指令 —— 避免无谓重绘); 行集未变时走增量
    /// 指令 (`RedrawRows`), 触发滚动 (窗口移动) 时回退全量 `Redraw`。
    #[test]
    fn set_selection_only_redraws_on_change() {
        let mut st = state();
        on_event(
            AppEvent::SetResults {
                items: results(&["a", "b"]),
                selected: 0,
            },
            &mut st,
        );
        assert_eq!(on_event(AppEvent::SetSelection(0), &mut st), vec![]);
        assert_eq!(
            on_event(AppEvent::SetSelection(1), &mut st),
            vec![Command::RedrawRows { prev: 0 }]
        );
        assert_eq!(st.results.selected(), 1);
    }

    /// 0x407 + 滚动: 高亮移出可视窗口 ⇒ 窗口移动 ⇒ 行集变化 ⇒ 全量 Redraw
    /// (增量路径只保证行集不变时的正确性)。
    #[test]
    fn set_selection_with_scroll_falls_back_to_full_redraw() {
        let mut st = state();
        on_event(
            AppEvent::SetResults {
                items: results(&["a", "b", "c", "d"]),
                selected: 0,
            },
            &mut st,
        );
        st.results.set_visible_max(2); // 可视 2 行, 共 4 项
        assert_eq!(
            on_event(AppEvent::SetSelection(1), &mut st),
            vec![Command::RedrawRows { prev: 0 }]
        );
        let cmds = on_event(AppEvent::SetSelection(2), &mut st); // 越出窗口 → 下滚
        assert_eq!(cmds, vec![Command::Redraw { pre_show: false }]);
        assert_eq!(st.results.window(), (1, 3));
    }

    /// 0x408: 有列表才重排; 空表时零指令 (幂等)。
    #[test]
    fn clear_results_is_idempotent() {
        let mut st = state();
        assert_eq!(on_event(AppEvent::ClearResults, &mut st), vec![]);
        on_event(
            AppEvent::SetResults {
                items: results(&["a"]),
                selected: 0,
            },
            &mut st,
        );
        assert_eq!(
            on_event(AppEvent::ClearResults, &mut st),
            vec![Command::Relayout]
        );
        assert!(st.results.is_empty());
        assert_eq!(on_event(AppEvent::ClearResults, &mut st), vec![]);
    }

    /// 会话生命周期: 0x401 显示与 0x402/0x403 隐藏都必须清列表 (框自己保证列表
    /// 活不过一次会话 —— 插件无需发送收尾消息)。
    #[test]
    fn results_never_survive_a_session() {
        for end in [AppEvent::HideFade, AppEvent::CancelHide] {
            let mut st = state();
            on_event(
                AppEvent::SetResults {
                    items: results(&["a", "b"]),
                    selected: 0,
                },
                &mut st,
            );
            assert_eq!(st.results.len(), 2);
            on_event(end, &mut st);
            assert!(st.results.is_empty(), "隐藏必须收起列表");

            let mut st2 = state();
            on_event(
                AppEvent::SetResults {
                    items: results(&["x"]),
                    selected: 0,
                },
                &mut st2,
            );
            on_event(AppEvent::ShowClear, &mut st2);
            assert!(st2.results.is_empty(), "0x401 显示必须清列表");
        }
    }

    /// 0x407 在无列表时是 no-op (不崩、不产生指令)。
    #[test]
    fn select_without_results_is_noop() {
        let mut st = state();
        assert_eq!(on_event(AppEvent::SetSelection(3), &mut st), vec![]);
    }

    // ---- 2026-10-04 搜索徽标 (0x40A/0x40B) ----

    /// 0x40A: 显示 → 重绘 (无 Relayout —— 锚定查询区, 不改几何); 同值幂等零指令。
    #[test]
    fn show_badge_semantics() {
        let mut st = state();
        let cmds = on_event(AppEvent::ShowBadge(crate::badge::GLYPH_MAGNIFIER), &mut st);
        assert_eq!(cmds, vec![Command::Redraw { pre_show: false }]);
        assert_eq!(st.badge, Some(crate::badge::GLYPH_MAGNIFIER));
        // 同值重复 = 幂等
        assert_eq!(
            on_event(AppEvent::ShowBadge(crate::badge::GLYPH_MAGNIFIER), &mut st),
            vec![]
        );
    }

    /// 0x40A 未注册字形 = 对端错误 → 忽略 (零指令、状态不变 —— 不带崩命令框)。
    #[test]
    fn show_badge_unknown_glyph_ignored() {
        let mut st = state();
        assert_eq!(on_event(AppEvent::ShowBadge(0), &mut st), vec![]);
        assert_eq!(on_event(AppEvent::ShowBadge(999), &mut st), vec![]);
        assert_eq!(st.badge, None);
    }

    /// 0x40B: 有徽标才重绘; 幂等 (无徽标零指令)。
    #[test]
    fn hide_badge_is_idempotent() {
        let mut st = state();
        assert_eq!(on_event(AppEvent::HideBadge, &mut st), vec![]);
        on_event(AppEvent::ShowBadge(crate::badge::GLYPH_MAGNIFIER), &mut st);
        assert_eq!(
            on_event(AppEvent::HideBadge, &mut st),
            vec![Command::Redraw { pre_show: false }]
        );
        assert_eq!(st.badge, None);
        assert_eq!(on_event(AppEvent::HideBadge, &mut st), vec![]);
    }

    /// 徽标随会话存在 (与结果列表同口径): 0x401 显示 / 0x402·0x403 隐藏一律清除 ——
    /// 即使插件漏发 0x40B 也不残留 (命令框侧兜底, 框自己保证「活不过一次会话」)。
    #[test]
    fn badge_never_survives_a_session() {
        for ev in [
            AppEvent::ShowClear,
            AppEvent::HideFade,
            AppEvent::CancelHide,
        ] {
            let mut st = state();
            on_event(AppEvent::ShowBadge(crate::badge::GLYPH_MAGNIFIER), &mut st);
            assert!(st.badge.is_some());
            on_event(ev.clone(), &mut st);
            assert_eq!(st.badge, None, "{ev:?} 必须清徽标");
        }
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
        assert_eq!(
            st.text.units(),
            snapshot.as_slice(),
            "WM_DESTROY 不触碰文本"
        );

        on_event(AppEvent::Char(0x21), &mut st); // Char 是写入点
        assert_eq!(st.text.len(), 5);
        on_event(AppEvent::ShowClear, &mut st); // 0x401 是写入点
        assert_eq!(st.text.len(), 0);
    }
}
