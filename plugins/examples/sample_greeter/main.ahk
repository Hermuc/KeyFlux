; sample_greeter —— 首个官方示例插件 (插件接入链路验证)。
; 最小插件契约: 入口函数接收按权限裁剪的 API 视图; permissions: [] → 仅 ui.* 可用。
; 卸载: 设置面板插件页删除, 或删除 data/plugins/sample_greeter/ 后重启引擎。
SampleGreeterMain(api) {
  api.Tip("KeyFlux 示例插件已加载")
}
