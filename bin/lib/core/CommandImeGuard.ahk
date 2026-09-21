/**
 * CommandImeGuard.ahk —— 命令框会话期间的输入法守卫 (2026-09-19 v4)。
 *
 * ============================ 目标 (用户明确需求) ============================
 *   1. 非搜索模式: 命令框**锁英文** —— 不因聚焦命令框而被系统切成中文, Shift 无法切中文。
 *   2. 搜索模式:   命令框**放开中文** —— 允许切换到中文输入法、输入/显示中文。
 *
 * ============================ 实现机制 ============================
 * 锁英文 = 历史形态 (InputHook 吞键 + 引擎投递英文 WM_CHAR): 命令框字符全部来自引擎投递,
 *   命令框线程的输入法从未被参与 ⇒ 无论输入法中/英都不影响, 不会变中文, 不新增输入法。
 *   这正是旧版本"自动英文"的机制 (dsg design-ime-guard.md §0)。
 *
 * 放开中文 = 运行时用 InputHook.KeyOpt 把文本键切到 "V" (可见/透传) + 停投递
 *   (CommandDisplay.SuppressKeycap := true): 物理键透传给命令框 → 其 IME 可组合/上屏中文。
 *   InputHook 构造时形态定死, 但 KeyOpt 支持**运行时**逐键修改 (v2 文档实证四类键)。
 *
 * ⚠ 架构边界 (如实说明, 见 design-ime-guard.md): 命令框透传时, 中文上屏以 WM_CHAR 直达
 *   命令框窗口、**不经 InputHook 回调** ⇒ Everything 插件的 onChar 只能拿到**按键**(英文/
 *   拼音), 拿不到上屏中文。故:
 *     - 命令框"能显示/能输入中文" —— 切透传可达成 ✓
 *     - Everything 的**中文检索匹配词** —— 仍靠"选中文字"(SelectionContext) ✓ (可靠通道)
 *   ☞ 因此"搜索模式允许中文输入"体现为命令框恢复中文输入法能力; 搜索匹配中文文件名请先选中。
 *
 * 与 ImeInputHost 的关系: ImeInputHost 恒置透传 (SuppressKeycap=true, hook 恒 V)。本模块在
 *   非搜索会话把 SuppressKeycap 覆盖为 false (历史锁英); 搜索时再置 true + KeyOpt V (放开中文)。
 *   注册顺序须在 ImeInputHost 之后 (本模块后生效)。
 *
 * 模块化/插件无关: 只依赖 CommandInputHooks + CommandDisplay (core)。EverythingSession
 *   触发搜索时调 UnlockForSearch(ih) 放开中文; 插件缺失时无此调用, 本模块零影响
 *   (InSession 由 OnSessionEnd 复位)。
 * provider 契约 (CONTRACTS §3.10): On* 回调必须 static; 抛异常只记日志, 不拖垮命令框。
 *
 * 快路径红线不适用: 本模块只在命令框输入期被调用。
 */
class CommandImeGuard {
  ; ---- 状态 (全 static; 方法内一律用 CommandImeGuard. 全名) ----
  static InSession := false      ; 是否会话中
  static SearchMode := false     ; 是否进入搜索模式

  ; ---- 需要放开中文的文本键 (字母/数字/空格; 拼音组合 + 候选确认所需) ----
  ; 含 {Backspace}: 搜索模式下退格既要删插件检索词、又要删命令框显示文本。
  ; 🔴 2026-09-21 订正: 命令框 exe 全二进制只有一处 WM_CHAR(0x0102) 比较点, 退格分支
  ;   (wParam 0x08) 嵌套其中; WM_KEYDOWN 比较点为 0 ⇒ 物理退格**不会**删除命令框文字。
  ;   故「删显示」只能靠引擎投递 WM_CHAR(0x08), 投递必须恒开 (见 CommandDisplay.
  ;   EchoBackspace 的注释)。本行的 {Backspace} 只决定物理键是否透传给窗口 (供 IME
  ;   组合期用), 与显示删除无关 —— 别再据此推断「投递可省」。
  static TEXT_KEYS := "a b c d e f g h i j k l m n o p q r s t u v w x y z" 
                   . " 0 1 2 3 4 5 6 7 8 9 {Space} {Backspace}"

  ; ---- CommandInputHooks provider 生命周期 ----

  /** 命令框显示前: 历史形态(吞键+投递英文), 锁英文, 不使用输入法。 */
  static OnSessionBegin() {
    CommandImeGuard.InSession := true
    CommandImeGuard.SearchMode := false
    ; 历史形态 = 投递通道开启、物理键不进 IME。覆盖 ImeInputHost 置的 true (透传),
    ; 因本 provider 注册在其后, 效果以本值优先 ⇒ 命令框锁英文。
    CommandDisplay.SuppressKeycap := false
  }

  /** 输入结束后: 复位状态 (不触碰输入法本身; 无泄漏)。 */
  static OnSessionEnd() {
    CommandImeGuard.InSession := false
    CommandImeGuard.SearchMode := false
  }

  /** 字符回调: 透明旁路 (本模块不消费字符)。 */
  static OnChar(ih, char, scope) {
    return false
  }

  /** 按键回调: 透明旁路 (锁英靠历史形态吞键, 放开靠 KeyOpt V, 无需本层拦键)。 */
  static OnKey(ih, vk, sc, scope) {
    return false
  }

  ; ---- 对外 API (供插件/命令体调用) ----

  /**
   * 进入搜索模式: 放开中文 —— 把文本键 KeyOpt 置 "V"(透传, 物理键进命令框 IME) + 停投递,
   * 使命令框能切换中文输入法、输入/显示中文。传入当前会话的 InputHook 对象 `ih`
   * (EverythingSession.OnChar 持有)。
   * 插件缺失时此调用不存在, 本模块零影响 (InSession 由 OnSessionEnd 复位)。
   */
  static UnlockForSearch(ih := 0) {
    if (!CommandImeGuard.InSession)
      return
    CommandImeGuard.SearchMode := true
    try {
      if (ih && ih.HasProp("KeyOpt")) {
        ih.KeyOpt(CommandImeGuard.TEXT_KEYS, "V")  ; 可见: 物理键透传
        ; 透传接管显示, 停投递防二次显示 (与 v4 恒透传一致)
        CommandDisplay.SuppressKeycap := true
      }
    }
  }
}