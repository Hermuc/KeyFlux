/**
 * ImeInputHost.ahk —— 命令框透传模式的启闭开关 (2026-09-19 v4)。
 *
 * ============================ 问题与结论 (证伪记录, 保留) ============================
 *   1. 「原软件对输入法有硬性限制」—— 证伪。全仓 DisableIME() 零调用点, exe 未导入
 *      imm32.dll, 命令框无子控件。
 *   2. 「皮肤 keyColor/keyOpacity 可以把八角框调没」—— 证伪。RTTI 显示
 *      SampleWindow::DrawKeys(..., ID2D1SolidColorBrush& brush, ...) 只收一支画刷,
 *      描边与字符同源; keyOpacity 是图层级 (置 0 连字符一起消失)。
 *   3. 「InputHook 结构上无法承载 IME」—— 表述过重, 真正根因更具体: capsHook 以
 *      InputHook("", ...) 创建, 无 V 选项 ⇒ **默认不可见 = 吞掉文本键**。物理键到不了
 *      命令框窗口, IME 上下文永远收不到键, 组合无从发生 (实测证据链见 CONTRACTS §3.11)。
 *      ⇒ 命令框显示 = 纯投递 WM_CHAR (0x0102), 实测上轮 SuppressKeycap 置位后字母
 *      全部消失即铁证。
 *   4. 「引擎进程能读 IME 组合串做浮层回显」—— 证伪。ImmGetContext 只能取本线程上下文,
 *      引擎进程永远看不到前台 IME 的组合 (浮层恒空)。浮层方案已整体废弃 (用户否决:
 *      要求字母直接显示在命令框内)。
 *   5. 🔴 「跨进程读 IME 开关状态做条件透传 (v3 设计)」—— **证伪, 即 v3 的死因**:
 *      HIMC 是进程本地句柄, 跨进程 ImmGetContext 恒返回 0 (kf_p1_ime_crossproc.ahk
 *      实测: 目标进程 notepad 同样 hIMC=0) ⇒ _QueryImeOpen 恒 -1 ⇒ SuppressKeycap 恒
 *      false ⇒ hook 恒为吞键形态 ⇒ 中文打不出 (2026-09-19 用户真机确认)。
 *   6. 🔴 「TSF 全局 compartment 读 IME 中英模式」—— **证伪**。AHK 内 DllCall 消费 TSF
 *      四连败: CLSID_TF_ThreadMgr REGDB_E_CLASSNOTREG → 改 TF_CreateThreadMgr 免 COM
 *      路径成功, 但 ITfThreadMgr::Activate 在 AHK 主线程 DllCall 直接挂死 (须跳过),
 *      GetGlobalCompartment (vtable idx14) 调用即抛异常 (kf_p2b/p2c/p2d 探针)。
 *      结论: 放弃一切 IME 状态预查, 透传恒开。
 *
 * ============================ 现行方案 (v4, CONTRACTS §3.12) ============================
 * **hook 恒 V (本模块启用时), 透传接管一切显示**:
 *   * 英文: 字母物理透传 → 命令框原生 WM_CHAR 显示 (exe 处理 0x0102 已证);
 *   * 中文: 拼音物理透传 → IME 原生组合/候选/上屏, 上屏中文以 WM_CHAR 直达命令框
 *     (非白名单, 数据 patch 后无框);
 *   * 投递通道整体关闭 (CommandDisplay.SuppressKeycap := true): EchoChar/EchoBackspace
 *     恒 no-op —— 物理键已原生显示/删除, 投递即二次;
 *   * 缩写匹配两形态恒开 (v4.2 恢复): 词表 (MatchList 全串) + FuzzySuffixFire (后缀)
 *     与历史形态同双通道 —— 用户裁决: 命令全英文字母, 中文意图仅在前置键触发之后,
 *     那时字符已被插件 OnChar 消费, 到不了匹配层; 搜索期 MatchList 的安全性由
 *     「全串匹配被空格前缀挡住」保障 (Input 形如 " se" ≠ "se");
 *   * providers 派发保留 (插件下拉/搜索功能不受透传影响)。
 *
 * 本类已退化为透传模式的**启闭开关**: Enable() 打开闸门 (OnSessionBegin 恒置
 * SuppressKeycap := true), Disable()/OnSessionEnd 复位 false —— 复位后 hook 由
 * MakeCapsHook 建为历史形态 (无 V + 原词表 + 投递显示), 自洽回退, 零其它改动。
 *
 * 依赖: CommandInputHooks (provider 契约), CommandDisplay (SuppressKeycap 落点)。
 * 快路径红线不适用: 本模块只在命令框输入期被调用。
 */

class ImeInputHost {
  ; 是否已启用 (模板默认启用; 未启用时命令框行为与历史一致)
  static Enabled := false

  ; 是否处于会话
  static InSession := false

  ; ---- 生命周期 (CommandInputHooks provider 接口) ----

  /**
   * 命令框显示前调用。启用时**恒置透传模式** —— 不查任何 IME 状态 (状态查询已整体
   * 证伪, 见头注释第 5/6 条; v3 的「按查询结果置位」正是中文打不出的直接死因)。
   *
   * 🔴 四个 On* 回调必须是 static —— CommandInputHooks.Register(ImeInputHost) 注册的是
   *    **类对象**, 而 AHK v2 类对象上实例方法的 HasProp 为 false (2026-09-19 实测:
   *    Foo.HasProp("Inst")=0 / Foo.HasProp("静态")=1), _Call 会静默跳过 ⇒ provider 永远
   *    不运行。全部状态都是 static, 方法体内一律用 ImeInputHost. 全名引用。
   */
  static OnSessionBegin() {
    if (!ImeInputHost.Enabled)
      return
    ImeInputHost.InSession := true
    ; 透传模式 = 投递通道整体关闭 (MakeCapsHook 据此建 V hook + 空词表)
    CommandDisplay.SuppressKeycap := true
  }

  /** 输入结束后调用: 复位透传 (不触碰用户输入法状态 —— 本模块只读不写)。 */
  static OnSessionEnd() {
    if (!ImeInputHost.Enabled)
      return
    ImeInputHost.InSession := false
    CommandDisplay.SuppressKeycap := false
  }

  /**
   * 每个未被消费的输入字符都会到这里 (本 provider 在分发链**末尾** —— 插件先注册,
   * 引擎后注册)。透传模式下无额外职责; 防御性透明旁路 (始终返回 false)。
   */
  static OnChar(ih, char, scope) {
    return false
  }

  /** 按键回调: 同上, 防御性透明旁路。始终返回 false。 */
  static OnKey(ih, vk, sc, scope) {
    return false
  }

  ; ---- 对外开关 ----

  /** 启用 (透传模式的唯一闸门: OnSessionBegin 据此置 SuppressKeycap)。 */
  static Enable() {
    this.Enabled := true
  }

  /** 停用并复位全部状态 (引擎退出/用户关掉该功能时调用)。 */
  static Disable() {
    this.Enabled := false
    this.InSession := false
    CommandDisplay.SuppressKeycap := false
  }
}
