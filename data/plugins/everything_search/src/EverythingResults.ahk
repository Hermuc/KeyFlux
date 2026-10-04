; ============================================================
; EverythingResults —— 结果列表视图 (端口层)。
;
; 2026-10-04 架构变更 (用户需求「列表必须是命令框**本体**的向下延伸, 与 Flow Launcher /
;   uTools 一致」): 旧实现自建一个独立 AHK Gui + ListView 浮在命令框下方, 再用
;   SetWindowRgn 的「耳朵」拼轮廓假装连体 —— 视觉上仍是两个组件 (用户报障「割裂」)。
;   命令框已是本仓库的 Rust 代码 (command-input/), 具备自绘能力 ⇒ 列表改由**命令框
;   自己渲染**: 窗口向下长高, 同一个圆角白框里 [查询区 | 分隔线 | 结果行] 一体呈现。
;   本文件随之退化为纯转发 + 文本口径的**视图端口** (渲染职责已移交命令框)。
;
; 职责边界:
;   * 入参 = items:Array<{path,name,isFolder}> + index (1 基; 0 = 无高亮);
;   * 唯一出参 = 经 EverythingHost 端口把数据推给命令框;
;   * 不自建窗口、不查 Everything、不读配置、不发键、不做 hit-test —— 鼠标命中判断在
;     命令框侧 (它才知道行几何), 经 0x409 回推给会话层 (见 EverythingSession.OnBoxNotify);
;   * 显示文本口径 = 完整路径 (命令框以 DT_PATH_ELLIPSIS 保留首尾, 长路径可读)。
;
; 消息契约 (与引擎协议 0x401-0x405 同构, 只加不改):
;   0x406 WM_COPYDATA (dwData = 'KFR1')  ← 整表推送 (EverythingHost.ShowResults)
;   0x407 (wParam = 0 基下标; -1 无高亮) ← 只移动高亮 (EverythingHost.SelectResult)
;   0x408                                ← 收起列表 (EverythingHost.ClearResults)
;   0x409 命令框 → 本进程              → 鼠标点选/悬停/滚轮 (InstallNotify 接收)
; ============================================================
#Warn All, Off

class EverythingResults {
  ; 命令框 → 引擎的回推消息号 (与 command-input/src/config.rs 的 APP_RESULTS_NOTIFY 同值)。
  static NOTIFY_MSG := 0x0409

  ; 命令框回推 (0x409) 的接收器状态。字段必须先落局部变量再调用 (AHK v2 里
  ; `Class.Field(...)` 会被按方法调用解析, 见 EverythingHost 顶部注释)。
  static _notifyInstalled := false
  static _target := 0

  /**
   * 安装命令框结果交互回推 (0x409) 的接收器。只安装一次; `ctrl` 随插件加载更新
   * (重载插件 = 换目标, 不重复注册)。@param ctrl EverythingController (需实现 OnBoxNotify)
   */
  static InstallNotify(ctrl) {
    EverythingResults._target := ctrl
    if (EverythingResults._notifyInstalled)
      return
    EverythingResults._notifyInstalled := true
    OnMessage(EverythingResults.NOTIFY_MSG, EverythingResultsNotifyForward)
  }

  /**
   * 显示结果列表。
   * @param items 结果数组 (每项 {path,name,isFolder})
   * @param index 高亮行 (1 基; 0 = 无高亮, 用于提示行)
   * @returns {Boolean} 是否成功推送给命令框
   */
  static Show(items, index) {
    return EverythingHost.ShowResults(EverythingResults.Lines(items), index)
  }

  /** 只移动高亮 (命令框**不会**回推本消息, 单向 —— 防回声环)。@param index 1 基; 0 = 无 */
  static Select(index) {
    return EverythingHost.SelectResult(index)
  }

  /** 收起列表 (命令框窗口回落基准高度)。 */
  static Hide() {
    return EverythingHost.ClearResults()
  }

  /** 单行提示 (无高亮): 打开失败等需要留在屏上的说明。 */
  static ShowHint(text) {
    return EverythingHost.ShowResults([text], 0)
  }

  /** 展示文本 = 完整路径; 路径为空时回落到名字 (口径与旧 ListView 列一致)。 */
  static Lines(items) {
    lines := []
    for it in items
      lines.Push((it.path != "") ? it.path : it.name)
    return lines
  }
}

/**
 * 文件级转发 (OnMessage 对类静态方法直传会报 ValueError: Invalid callback —— AHK v2 实测;
 * 自由函数 / Bind / ObjBindMethod 均可, 与旧 ED_WheelForward 同款绕行)。
 * lParam: 1 = 行被点选 (打开), 2 = 高亮变化 (悬停/滚轮)。
 */
EverythingResultsNotifyForward(wParam, lParam, msg, hwnd) {
  t := EverythingResults._target
  if (IsObject(t))
    t.OnBoxNotify(wParam, lParam)
  return 0
}
