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

  ; 命中命令的「待收尾」状态 (2026-09-20): 模糊命中路径 (FuzzySuffixFire) 只记录命中并停钩,
  ; 执行与隐藏由 EnterCapslockAbbr 延后做 (见 FinishDelayMs)。全串命中 (MatchList) 路径
  ; 不写这里 —— 它的命中文本由 capsHook.Match 提供。
  static PendingAbbr := ""
  static PendingScope := ""

  ; 命中后的收尾延迟 (ms)。
  ; 🔴 依据 (2026-09-20 用户真机实测): 命中这一击的字符 (终止字符) 不再被原生显示,
  ; 它靠 `CommandDisplay.EchoTerminalChar` 补投 WM_CHAR; 而「投递 → 命令框处理 → 绘制」
  ; 需要 1~2 个绘制周期, 若投完立刻执行+隐藏, 该字符可能来不及上屏。
  ; 故延后这么久再执行命令并隐藏命令框 —— 取值 = 约 2 帧 (60Hz 下 1 帧 16.7ms):
  ; 既保证字符上屏, 又让「执行」在用户感知上仍是「打完即执行」(首版 150ms 被用户判为太慢)。
  ; 语义不变: 命令仍无需用户确认即执行。要更快可下调, 但 ≤1 帧有丢字符风险 (勿设 0)。
  static FinishDelayMs := 30

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
    ; 复位上一会话的待收尾状态: 延后收尾的回调可能跨越会话边界返回, 会话间不泄漏
    this.PendingAbbr := ""
    this.PendingScope := ""
    this.SessionActive := true
    this._Notify("OnSessionBegin")
  }

  /** 输入结束后调用: 通知全部 provider 收尾 (复位会话状态等)。 */
  static EndSession() {
    this.SessionActive := false
    this._Notify("OnSessionEnd")
  }

  /**
   * 取走模糊命中的待收尾命令 (一次性消费: 取出即清空, 防重复执行)。
   * @returns {Array|string} [scope, abbr]; 空串 = 无待收尾
   */
  static TakePending() {
    if (this.PendingAbbr = "")
      return ""
    out := [this.PendingScope, this.PendingAbbr]
    this.PendingScope := ""
    this.PendingAbbr := ""
    return out
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

  ; 调用 provider 方法: 未实现该方法时返回 false (不抛错, 免日志噪声)。
  ;
  ; 🔴 必须用「动态名直接调用」写法 p.%name%(args*) —— 不可写成
  ;    `fn := p.%name%` 再 `fn.Call(args*)`:
  ; AHK v2 的 `obj.Method` 取到的是**未绑定 this** 的函数对象 (this 只是普通首参, 取值前无值;
  ; 与 Python/JS 的 bound method 语义相反)。于是 `.Call(args*)` 会把首个实参顶替成 this,
  ; 并令末位实参缺失 ⇒ 每次回调都抛 `Missing a required parameter.`, 而该异常会被
  ; DispatchChar/DispatchKey/_Notify 的 try/catch 吞掉并「视为未消费」⇒ provider 从未真正执行,
  ; 症状是「插件像没挂上」——命令框按前置触发键完全无反应, 且 /Validate 与 lint 都查不出
  ; (纯运行时语义)。2026-09-19 实测: everything_search 插件按空格无反应即此因, 见
  ; tools/command_input_hooks_test.ahk (回归探针) 与 Makefile 的 check-hooks 目标。
  ; 同仓库先例: bin/lib/Monitor.ahk:363 `this.%GetMethodName%(hPhysicalMonitor, params*)`。
  static _Call(p, name, args*) {
    if (!p.HasProp(name))
      return false
    return p.%name%(args*)
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
 * 命令框 OnChar 入口 (模板 keyflux.tmpl 绑定)。
 *
 * 显示与匹配双通道 (§3.12 v4.2, 2026-09-19):
 *   * 字符显示: 透传模式下物理键已原生直显, EchoChar 经 ShouldEcho 兑停 (no-op);
 *     历史形态照常投递。
 *   * 缩写匹配: FuzzySuffixFire **恒跑** (v4 的透传旁路已撤) —— 用户裁决: 设置面板
 *     的命令全部由英文字母组成, 命令框内需要输入中文的唯一场景是前置键 (如空格)
 *     触发插件之后, 而那时字符已被插件 OnChar 消费 (DispatchChar 提前 return),
 *     根本到不了本匹配层 ⇒ v4 假设的「拼音后缀误触发」场景不存在。搜索期
 *     MatchList 的安全性由「全串匹配被空格前缀挡住」保障 (检索词 Input 形如
 *     " se" ≠ "se"); providers 派发仍最先 (插件消费即不进匹配层)。
 *   * (v4 的「providers 派发保留」维持不变; Match 分支的 EchoChar 同样被 ShouldEcho
 *     兑停, 防二次显示。)
 *
 * 🔴 EchoChar 与 FuzzySuffixFire 必须 try 包裹 (2026-09-19 加固): 二者在 InputHook 回调
 * 线程上直接运行, 任何异常都无 try/catch 兜底 —— 后果是 AHK 错误对话框 + 线程死亡 +
 * StartInputHook 已执行的 Suspend(true) 永远不会恢复 ⇒ 全部热键失效 (用户视角:
 * "报错弹窗 + 命令框卡死, 无法关闭也无法输入")。实测 PostMessage 到已消失的命令框
 * 窗口会抛 TargetError (kf_diag2: "PostMessage 抛出: TargetError: Target window not
 * found."), 正是此路径。provider 派发已有逐个 try (DispatchChar), 这里补齐剩余两段。
 *
 * 🔴 命中后的收尾是**延后**的 (2026-09-20): 见 CommandInputHooks.FinishDelayMs ——
 * Match 与模糊命中都不再当场执行/隐藏, 先让命令框把终止字符画出来 (旧行为当场执行+隐藏
 * 会让该字符永远显示不出来)。模糊命中在本函数里只写 Pending* 并停钩, 执行交给编排层。
 */
CommandInputOnChar(ih, char, scope) {
  if (CommandInputHooks.DispatchChar(ih, char, scope))
    return
  ; 回显统一过 CommandDisplay: 透传模式下 ShouldEcho 全停 (no-op), 历史形态照常投递
  try CommandDisplay.EchoChar(ih, char)
  catch as e
    CommandInputHooks._log("EchoChar 异常: " e.Message)
  ; 缩写匹配恒跑 (v4.2): 逐字符后缀查表, 命中即执行 —— 与历史形态行为一致
  ; (透传模式词表已恢复, MatchList 全串精确 + 本函数后缀模糊, 双通道同历史)
  try FuzzySuffixFire(ih, char, scope)
  catch as e
    CommandInputHooks._log("FuzzySuffixFire 异常: " e.Message)
}

/**
 * 命令框 OnKeyDown 入口 (模板 keyflux.tmpl 绑定)。
 * v4 起无整体守卫: providers 派发照常 (透传模式下插件导航/触发仍是有效功能);
 * 退格投递**恒开** (CommandDisplay.EchoBackspace 已去掉 SuppressKeycap 分支)。
 *
 * 🔴 2026-09-21 订正 (原注释称「物理退格已随 V hook 透传并原生删除, 投递即二次删除」,
 *   该前提经静态反汇编证伪): 命令框 exe 全二进制只有一处 WM_CHAR(0x0102) 比较点
 *   (`81 fa 02 01 00 00` @ 0x9687), 退格分支 (`66 83 fe 08`, wParam 0x08) 嵌套其中;
 *   WM_KEYDOWN(0x0100) 比较点为 0。⇒ 物理退格不产生删除, 投递是**唯一**删除来源,
 *   拦停即「文字已删但命令框还显示」。用户真机报障即此因。
 *
 * 先给 provider 机会消费, 未消费则由本处补投退格。历史实现是无条件调
 * PostBackspaceToCaspAbbr, 但当时只有 {Backspace} 被 KeyOpt("{Backspace}", "N") 通知到,
 * 等价于「只有退格会走到这里」; 现在 Up/Down/Enter 也参与通知 (供插件下拉列表导航),
 * 故必须按 vk 分流, 避免方向键被当成退格投递。
 */
CommandInputOnKeyDown(ih, vk, sc, scope) {
  if (CommandInputHooks.DispatchKey(ih, vk, sc, scope))
    return
  if (vk = 0x08) {
    try CommandDisplay.EchoBackspace(ih, vk, sc)
    catch as e
      CommandInputHooks._log("EchoBackspace 异常: " e.Message)
  }
}
