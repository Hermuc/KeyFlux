; ============================================================
; FolderHistory.ahk —— QuickSwitch 内三层之「唯一 Shell COM」层。
;
; 唯一职责: 通过 Shell.Application 枚举资源管理器窗口的当前目录。
; 成本: 单次枚举实测约 16 ms (阶段 0 §3.5) —— 调用方必须确保它只在
;       定时器/异步路径执行, 绝不进入热键回调 (设计 §7.2 #6)。
; 错误隔离: 所有 COM 调用 try/catch, 失败静默返回空/不更新, 绝不抛进热键链路。
; 依赖方向: 本层不依赖任何其他 quickswitch 模块。
; ============================================================
#Warn All, Off

; 最近一次进入前台的资源管理器目录 (由 HistNoteForegroundExplorer 更新)。
class FolderHistoryState {
  static LastForegroundDir := ""
}

; 枚举所有资源管理器窗口的当前目录, 返回去重后的路径数组。
HistCollectExplorerDirs() {
  dirs := []
  seen := Map()
  try {
    shell := ComObject("Shell.Application")
    for win in shell.Windows {
      try {
        p := win.Document.Folder.Self.Path
        if (p != "" && !seen.Has(p)) {
          seen[p] := true
          dirs.Push(p)
        }
      }
    }
  } catch {
    ; 静默: 枚举失败不阻断任何上层流程
  }
  return dirs
}

; 记录"当前前台窗口(若为资源管理器)的目录"。
; 供定时器周期调用; 调用方负责保证不落在热键回调里。
HistNoteForegroundExplorer() {
  try {
    fg := WinActive("A")
    if (!fg) {
      return
    }
    shell := ComObject("Shell.Application")
    for win in shell.Windows {
      try {
        if (win.HWND = fg) {
          p := win.Document.Folder.Self.Path
          if (p != "") {
            FolderHistoryState.LastForegroundDir := p
          }
        }
      }
    }
  } catch {
    ; 静默
  }
}

; 返回"最近一次进入前台的资源管理器目录"; 无记录时返回空串。
HistLastForegroundExplorerDir() {
  return FolderHistoryState.LastForegroundDir
}
