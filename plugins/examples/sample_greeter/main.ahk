; sample_greeter —— 首个官方示例插件 (演示三项插件能力)。
; ① 入口函数: 接收按权限裁剪的 API 视图 (permissions: [] → 仅 ui.*)
; ② IAction 运行时动作: 注册 Type="plugin:sample_greeter:timestamp" (契约 §3.2/§3.4)
; ③ 贡献行为包: behaviors/sample_timestamp/behavior.json 引用上述动作
;    (选中动作里选「示例: 复制时间戳」即可触发)
; 卸载: 设置面板插件页删除, 或删除 data/plugins/sample_greeter/ 后重启引擎。
global __sampleGreeterAPI := ""

SampleGreeterMain(api) {
    global __sampleGreeterAPI
    __sampleGreeterAPI := api
    api.Tip("KeyFlux 示例插件已加载")
    if (!ActionRegistry.Register(SampleGreeterTimestampAction()))
        throw Error("IAction 注册被拒绝 (重复 Type 或 Validate 未过)")
}

class SampleGreeterTimestampAction {
    Type := "plugin:sample_greeter:timestamp"
    CanRunInAbbr := false

    Validate() {
        return ""
    }

    Execute(ctx) {
        global __sampleGreeterAPI
        fmt := FormatTime(, "yyyy-MM-dd HH:mm:ss")
        A_Clipboard := fmt
        __sampleGreeterAPI.Tip("时间戳已复制: " fmt)
    }

    Preview(ctx) {
        return "复制当前时间戳到剪贴板"
    }
}
