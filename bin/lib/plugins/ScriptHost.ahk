/**
 * ScriptHost —— 子进程长任务执行 (docs/CONTRACTS.md §3.7)。
 * 阶段 5 落地基础实现: 委托 Run/RunWait 启动子进程,
 * 承接现 RunScriptWithSelected 的执行方式。
 * 红线: 长任务必须走子进程, 不得阻塞主进程回调 (约束 4, 回调 ≤50ms)。
 */
class ScriptHost {
  /**
   * 子进程执行脚本/程序。
   * @param scriptPath 脚本或程序路径
   * @param args 附加参数 (原样拼接)
   * @param inBackground true = 异步启动 (Run); false = 等待结束 (RunWait)
   * @return true = 启动成功; false = 失败 (路径不存在等, 错误隔离不抛出)
   */
  static Run(scriptPath, args := "", inBackground := true) {
    cmd := '"' scriptPath '"'
    if (args != "")
      cmd .= " " args
    try {
      if (inBackground)
        Run(cmd)
      else
        RunWait(cmd)
      return true
    } catch {
      return false
    }
  }
}
