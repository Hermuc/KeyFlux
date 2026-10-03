; ============================================================
; EverythingHost —— 引擎依赖的**唯一端口** (Ports & Adapters)。
;
; 为什么要有这一层: 本插件与引擎的耦合原本散落在 3 个文件、13 处直接引用
;   (CommandInputHooks / CommandDisplay / CommandImeGuard / SelectionContext /
;   SysLangIsChinese)。引擎改任何一个符号名, 都要全插件 grep。收敛到本类后:
;   * 引擎 API 演进 = 只改本文件;
;   * 插件其余部分只认识 EverythingHost, 不知道引擎存在;
;   * 探针/单测可整缝替换 (Impl), 不再需要逐个 stub 引擎全局类
;     (先例 = EverythingExplorerRunner.Impl, 见其注释)。
;
; 替换纪律 (与 EverythingExplorerRunner 同款):
;   * Impl = 0           → 走原生引擎全局 (生产路径);
;   * Impl = <对象>      → 同名方法派发到该对象 (探针记录/断言)。
;   🔴 类静态字段必须先落到局部变量再调用: `Class.Field(...)` 在 AHK v2 里按**方法调用**
;     解析, 字段值是函数对象时会报 "Too many parameters passed to function." (实测 2.0.19)。
;
; 原生方法**不做** try/catch: 引擎符号缺失应尽早炸出来 (加载/注册期), 而不是被静默吞掉 ——
; 与「插件出错由 PluginManager 隔离」的口径一致。
; ============================================================

class EverythingHost {
  static Impl := 0        ; 0 = 未替换 (走引擎全局); 其余 = 同名方法的可调用对象

  /** 注册命令框输入钩子提供者。@returns 引擎 Register 的返回值 (false = 拦截点缺失/重复注册)。 */
  static RegisterCommandHook(ctrl) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.RegisterCommandHook(ctrl)
    return CommandInputHooks.Register(ctrl)
  }

  /** 把前台切回会话开始时的窗口 (取选中文字前调用, 见 Session.SeedFromSelection 注释)。 */
  static ActivateBackend() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.ActivateBackend()
    CommandInputHooks.ActivateBackend()
  }

  /** 命令框视觉回显一个字符 (透传模式下物理键不经命令框, 显示靠投递)。 */
  static EchoChar(ih, char) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.EchoChar(ih, char)
    CommandDisplay.EchoChar(ih, char)
  }

  /** 命令框视觉回显退格。 */
  static EchoBackspace(ih, vk, sc) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.EchoBackspace(ih, vk, sc)
    CommandDisplay.EchoBackspace(ih, vk, sc)
  }

  /** 把前台还给命令框 (取完选中文字后调用; 失败只损失显示, 搜索路径不依赖焦点)。 */
  static ActivateCommandWindow() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.ActivateCommandWindow()
    CommandDisplay.ActivateCommandWindow()
  }

  /** 进入搜索模式时放开中文输入 (KeyOpt 文本键透传, 允许中文检索)。 */
  static UnlockForSearch(ih) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.UnlockForSearch(ih)
    CommandImeGuard.UnlockForSearch(ih)
  }

  /**
   * 取当前选中内容。
   * @param wait 传给引擎 SelectionContext.Get (true = 必要时等待选区就绪)
   * @returns {Object} {type: "text"|"file", content: String}
   */
  static GetSelection(wait) {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.GetSelection(wait)
    return SelectionContext.Get(wait)
  }

  /** 系统语言是否中文 (插件词表的语言判定依据)。首调固定的语义由调用方 (Messages) 持有。 */
  static IsChinese() {
    impl := EverythingHost.Impl
    if (IsObject(impl))
      return impl.IsChinese()
    return SysLangIsChinese()
  }
}
