/**
 * CommandInputHooks —— 命令框 (KeyFlux-CommandInput) 输入期的可插拔拦截点。
 *
 * 背景: 命令框本体是上游预编译二进制 (bin/KeyFlux-CommandInput.exe, 无源码), 它只做
 * 「按键镜像显示」; 真正的键盘捕获在主进程的 InputHook (见 core/AbbrInput.ahk)。
 * 因此任何「输入期间的新交互」(例如插件按下前置键唤起下拉列表) 都只能挂在 InputHook 的
 * OnChar / OnKeyDown 上 —— 本文件就是那一层稳定的扩展点。
 *
 * 契约 (为什么要过这层而不是各处硬接):
 *   * 引擎只认「顺序调用一组 provider」, provider 由插件注册, 引擎不 import 任何插件;
 *   * provider 返回 true = 已消费本次按键 (引擎不再投递字符到命令框, 也不做缩写模糊匹配);
 *     返回 false = 引擎按原有语义处理 (字节原样, 零行为变更);
 *   * provider 抛异常只记日志并视为「未消费」, 不拖垮命令框 (错误隔离, 与 PluginManager 同策略);
 *   * 生命周期: BeginSession (命令框显示前) / EndSession (输入结束后), 供 provider 备/收状态。
 *
 * 快路径红线不适用: 本模块只在命令框输入期被调用, 不在重映射/发键/鼠标快路径上。
 */

class CommandInputHooks {
  ; 已注册的 provider 列表 (按注册顺序调用, 先到者有机会先消费)
  static Providers := []

  ; 命令框显示前的前台窗口句柄 (取选中文字需要它重新成为前台, 见 ActivateBackend)
  static BackendWindow := 0

  ; 当前是否处于命令框输入会话
  static SessionActive := false

  /**
   * 注册 provider。provider 为对象/类实例, 可实现以下方法 (全部可选, 缺失即视为不处理):
   *   OnSessionBegin()                       命令框显示前
   *   OnSessionEnd()                         输入结束后 (含匹配命中 / Esc / 超时)
   *   OnChar(ih, char, scope) -> bool        返回 true 表示消费该字符
   *   OnKey(ih, vk, sc, scope) -> bool       返回 true 表示消费该按键
   * @returns {boolean} false = 已注册过 (幂等)
   */
  static Register(provider) {
    for p in this.Providers {
      if (p = provider)
        return false
    }
    this.Providers.Push(provider)
    return true
  }

  /** 注销 provider (幂等)。 */
  static Unregister(provider) {
    i := 1
    while (i <= this.Providers.Length) {
      if (this.Providers[i] = provider) {
        this.Providers.RemoveAt(i)
        return true
      }
      i += 1
    }
    return false
  }

  /**
   * 命令框显示前调用: 记下当前前台窗口 (随后命令框可能抢走前台, 取选中文字要切回去),
   * 并通知全部 provider 开始新会话。
   */
  static BeginSession() {
    this.BackendWindow := 0
    try this.BackendWindow := WinExist("A")
    this.SessionActive := true
    this._Notify("OnSessionBegin")
  }

  /** 输入结束后调用: 通知全部 provider 收尾 (隐藏浮层等)。 */
  static EndSession() {
    this.SessionActive := false
    this._Notify("OnSessionEnd")
  }

  /**
   * 需要时把前台切回会话开始时的窗口 (命令框抢了前台时), 供「取当前选中文字」使用。
   * @returns {boolean} true = 当前前台已可用于发送 Ctrl+C
   */
  static ActivateBackend() {
    if (this.BackendWindow = 0 || !WinExist(this.BackendWindow))
      return false
    try {
      if (WinActive("ahk_class MyKeymap_Command_Input"))
        WinActivate(this.BackendWindow)
    } catch {
      return false
    }
    return true
  }

  /** 逐 provider 分发字符; 返回 true = 已被某 provider 消费。 */
  static DispatchChar(ih, char, scope) {
    for p in this._Snapshot() {
      try {
        if (this._Call(p, "OnChar", ih, char, scope))
          return true
      } catch as e {
        this._log("provider OnChar 异常: " e.Message)
      }
    }
    return false
  }

  /** 逐 provider 分发按键; 返回 true = 已被某 provider 消费。 */
  static DispatchKey(ih, vk, sc, scope) {
    for p in this._Snapshot() {
      try {
        if (this._Call(p, "OnKey", ih, vk, sc, scope))
          return true
      } catch as e {
        this._log("provider OnKey 异常: " e.Message)
      }
    }
    return false
  }

  ; ---- 内部 ----

  ; 调用 provider 方法: 未实现该方法时返回 false (不抛错, 免日志噪声)
  static _Call(p, name, args*) {
    if (!p.HasProp(name))
      return false
    fn := p.%name%
    return fn.Call(args*)
  }

  static _Notify(name) {
    for p in this._Snapshot() {
      try {
        this._Call(p, name)
      } catch as e {
        this._log("provider " name " 异常: " e.Message)
      }
    }
  }

  ; 复制一份再遍历: provider 可能在回调里注册/注销自己
  static _Snapshot() {
    out := []
    for p in this.Providers
      out.Push(p)
    return out
  }

  static _log(msg) {
    try {
      FileAppend(FormatTime(, "yyyy-MM-dd HH:mm:ss") " " msg "`n", "logs\command_input_hooks.log")
    }
  }
}

/**
 * 命令框 OnChar 入口 (模板 keyflux.tmpl 绑定): 先给 provider 机会消费, 未消费则保持
 * 原有语义 —— 字符投递到命令框 + 逐字符后缀模糊匹配 (顺序与历史实现完全一致)。
 */
CommandInputOnChar(ih, char, scope) {
  if (CommandInputHooks.DispatchChar(ih, char, scope))
    return
  PostCharToCaspAbbr(ih, char)
  FuzzySuffixFire(ih, char, scope)
}

/**
 * 命令框 OnKeyDown 入口 (模板 keyflux.tmpl 绑定): 先给 provider 机会消费, 未消费则仅对
 * 退格保持原语义。历史实现是无条件调 PostBackspaceToCaspAbbr, 但当时只有 {Backspace} 被
 * KeyOpt("{Backspace}", "N") 通知到, 等价于「只有退格会走到这里」; 现在 Up/Down/Enter 也
 * 参与通知 (供插件下拉列表导航), 故必须按 vk 分流, 避免方向键被当成退格投递。
 */
CommandInputOnKeyDown(ih, vk, sc, scope) {
  if (CommandInputHooks.DispatchKey(ih, vk, sc, scope))
    return
  if (vk = 0x08)
    PostBackspaceToCaspAbbr(ih, vk, sc)
}
