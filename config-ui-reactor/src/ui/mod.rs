//! UI 层：块模型/状态 → WinUI 控件的构建器。
//!
//! 与 `services` 的分工：`services` 只做纯逻辑（解析/契约/HTTP），**不含任何控件**；
//! 本层负责把纯逻辑产物翻译成 reactor `View`。这样纯逻辑可被单元测试直接覆盖，
//! 而视图构造保持「薄」——避免旧版 `MarkdownRenderer` 那种解析与控件深度交织。

pub mod abbr_view;
pub mod action_editor;
pub mod keymap_view;
pub mod markdown_view;
pub mod plugins_view;
pub mod selected_action_view;
