; ============================================================
; EverythingSettings —— 插件设置的读取与归一。
; 存储: 引擎的 ConfigProvider (data/plugin-settings.json, 键空间按 "<pluginId>:<key>" 隔离),
;       经 APIView.GetSetting/SetSetting 访问 (需 settings 权限)。
; 本文件只负责「默认值 + 归一 + 落点推导」, 不做任何 IO。
; ============================================================

class EverythingSettings {
  ; 归一后的设置值
  static TriggerKey := " "    ; 前置触发键 (单字符)
  static EverythingPath := ""
  static EsPath := ""
  static Limit := 20

  ; 插件自身目录下的 es.exe 落点 (插件包自带二进制时优先可用)
  static SelfEsPath := ""

  /**
   * 从 API 视图读取全部设置。
   *
   * 可反复调用 (设置热重载): 入口加载一次给引擎启动后的首会话用, 之后每次命令框会话
   * 开始时再调一次, 于是设置面板里改完的值**无需重启引擎**即刻生效 ——
   * plugin-settings.json 只是一个几行的扁平文件, 4 次 Get 的代价可忽略。
   *
   * @param api 按 permissions 裁剪的 API 视图 (需 settings 权限)
   * @returns {Boolean} 是否有值发生变化 (调用方据此决定要不要让通道探测缓存失效 ——
   *   EverythingProviders.ResolveEs 的 PATH 兜底会起一个子进程, 无条件失效等于每次
   *   会话多一次无谓 spawn)
   */
  static Load(api) {
    key := this.NormKey(api.GetSetting("triggerKey"))
    ep := this.NormPath(api.GetSetting("everythingPath"))
    esp := this.NormPath(api.GetSetting("esPath"))
    lim := this.NormLimit(api.GetSetting("limit"))
    changed := (key != this.TriggerKey) || (ep != this.EverythingPath)
            || (esp != this.EsPath) || (lim != this.Limit)

    this.TriggerKey := key
    this.EverythingPath := ep
    this.EsPath := esp
    this.Limit := lim
    this.SelfEsPath := A_ScriptDir "\..\data\plugins\everything_search\bin\es.exe"
    return changed
  }

  /**
   * 触发键归一: 单个可打印字符 (默认空格)。
   * 只取首字符 —— 用户可能填了 "空格" 之类描述文本, 取首字符会得到 "空", 但那不是可打印
   * 触发语义; 故仅当首字符为可打印 ASCII 时才采用, 否则回落空格 (与默认值一致)。
   */
  static NormKey(v) {
    if (v = "")
      return " "
    c := SubStr(v, 1, 1)
    if (Ord(c) < 32 || Ord(c) > 126)
      return " "
    return c
  }

  /** 路径归一: 去首尾空白与可能被粘贴进来的双引号。 */
  static NormPath(v) {
    return Trim(StrReplace(v, '"'), " `t`r`n")
  }

  /** 结果条数归一: 1-100, 非法值回落 20 (非数字输入不能让算术抛异常)。 */
  static NormLimit(v) {
    if (v = "" || !IsNumber(v))
      return 20
    n := Integer(v)
    if (n < 1 || n > 100)
      return 20
    return n
  }
}
