; ============================================================
; EverythingSearch —— 编排层: 保证 Everything 已运行 -> 选通道 -> 查询 -> 失败重试。
; 本层不做 UI, 也不解析结果格式 (那是通道层的事); 只处理「时序与降级」。
; ============================================================

class EverythingSearch {
  static ProcessName := "Everything.exe"
  static LaunchWaitMs := 6000     ; 自动拉起后最多等这么久 (冷启动要读 db)
  static LaunchPollMs := 250
  static RetryDelayMs := 400      ; 首次查询失败后的重试间隔 (数据库加载竞态)

  /** Everything 进程是否在运行 (不依赖 IPC 窗口, 避免版本差异)。 */
  static IsRunning() {
    return ProcessExist(this.ProcessName) ? true : false
  }

  /**
   * 保证 Everything 已运行: 未运行则按配置路径拉起并轮询等待。
   * @returns {boolean} false = 未运行且无法拉起 (未配置路径 / 路径无效 / 启动失败)
   */
  static EnsureRunning() {
    if (this.IsRunning())
      return true

    exe := EverythingSettings.EverythingPath
    if (exe = "" || !FileExist(exe))
      return false

    try {
      Run('"' exe '"')
    } catch {
      return false
    }

    waited := 0
    while (waited < this.LaunchWaitMs) {
      Sleep this.LaunchPollMs
      waited += this.LaunchPollMs
      if (this.IsRunning())
        return true
    }
    return false
  }

  /**
   * 执行一次搜索 (同步; 调用方在命令框输入期, 单次 es 调用约 10-60ms)。
   * @param query 检索词
   * @param limit 结果条数上限
   * @returns {{ok:Boolean, error:String, items:Array<{path,name,isFolder}>}}
   */
  static Run(query, limit) {
    if (Trim(query, " `t`r`n") = "")
      return {ok: false, error: ES_ERR_EMPTY, items: []}

    if (!this.EnsureRunning()) {
      ; 未配置路径时给出更精确的提示 (区分「没配」与「配了但拉不起来」)
      err := (EverythingSettings.EverythingPath = "") ? ES_ERR_NO_PATH : ES_ERR_NOT_RUNNING
      return {ok: false, error: err, items: []}
    }

    provider := EverythingProviders.Create()
    res := provider.Search(query, limit)

    ; 冷启动竞态: Everything 进程已在但数据库仍在加载, es 可能返回空/失败 —— 重试一次
    if (!res.ok && (res.error = ES_ERR_NOT_RUNNING || res.error = ES_ERR_QUERY)) {
      Sleep this.RetryDelayMs
      res := provider.Search(query, limit)
    }
    return res
  }

  /** 供 UI 提示: 当前生效的通道名。 */
  static ProviderName() {
    return EverythingProviders.Create().Name
  }
}
