/**
 * 内置动作聚合入口。
 * 模块化重构阶段 3: 原单文件实现已按 TypeID 拆入 builtins/, 每类一文件,
 * 函数体逐行搬运未做修改。本文件保留原 include 路径 (模板引用
 * lib/actions/Actions.ahk), 生成产物与拆分前完全一致。
 * AHK 中函数为全局定义, include 顺序不影响可见性。
 *
 * 注意: 本聚合**有意不包含** ActionRegistry / IAction / IRegistration ——
 * 注册表服务插件动作, 尚未接入生成脚本 (零行为变更原则);
 * 内置动作仍由 Go 生成端编译期直连渲染 (快路径红线, 见 IAction.ahk 头注)。
 */
#Include builtins\type1_activate_or_run.ahk
#Include builtins\type2_system.ahk
#Include builtins\type3_window.ahk
#Include builtins\type4_mouse.ahk
#Include builtins\type6_send_keys.ahk
#Include builtins\type7_text_features.ahk
#Include builtins\type8_builtin.ahk
#Include builtins\type9_keyflux.ahk
