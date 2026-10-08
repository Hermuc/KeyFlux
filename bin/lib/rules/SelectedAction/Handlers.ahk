; ============================================================
; SelectedAction —— 内容处理器（顶层函数，非 class 成员）
;
; 由门面 SelectedAction.ahk 在**顶层** #Include。磁力链接/注册表/命令替换/脚本文本。
; ============================================================


/**
 * 用默认 BT 下载工具下载磁力链接 (走 magnet: 协议关联, 不硬编码具体下载软件)
 * 系统未注册默认处理器时给出中文提示而非静默失败
 * @param content 选中内容
 */
DownloadMagnet(content) {
  line := Trim(StrSplit(content, "`n")[1])
  if not (line) {
    return
  }
  if not (CheckMagnetHandler()) {
    Tip(Translation().magnet_no_handler, -2500)
    return
  }
  Run(line)
}

/**
 * 检测系统是否注册了 magnet: 协议默认处理器
 * HKCR 为 HKLM/HKCU 类注册的合并视图, 普通用户权限可读
 * @returns {boolean}
 */
CheckMagnetHandler() {
  try {
    cmd := RegRead("HKCR\magnet\shell\open\command")
    return cmd != ""
  }
  catch {
    ; 未注册 magnet: 是**正常状态** (返回 false 即正确答案), 不是故障 ⇒ 刻意不记日志。
    ; 读不到 HKCR 才会落到此处 (权限受限), 但本函数可能被高频调用, 记日志会刷屏。
    ; 2026-10-07 审查: 把「此处不记」的理由写进代码, 而不是留一个无解释的空 catch。
    return false
  }
}

/**
 * 打开注册表编辑器并定位到选中键路径
 * 原理: regedit 启动时读取 LastKey 值自动定位 (系统内置行为, 无需第三方工具与管理员权限)
 * regedit 已在运行时先结束再重开 (regedit 无未保存数据, 杀进程安全)
 * @param content 选中内容
 */
OpenRegistryKey(content) {
  path := Trim(StrSplit(content, "`n")[1])
  if not (path) {
    return
  }
  try RegWrite(path, "REG_SZ", "HKCU\Software\Microsoft\Windows\CurrentVersion\Applets\Regedit", "LastKey")
  catch as err {
    ; 已有用户可见提示, 再补一条留痕: 弹窗转瞬即逝, 事后无从复盘「为什么没打开注册表」。
    EngineLogWarn("SelectedAction.OpenRegistryKey: 写 LastKey 失败", "path=" path " err=" err.Message)
    Tip(Translation().registry_open_failed, -2500)
    return
  }
  if ProcessExist("regedit.exe") {
    ProcessClose("regedit.exe")
    ProcessWaitClose("regedit.exe", 2)
  }
  Run("regedit.exe")
}

/**
 * 把 %selected% 替换为选中内容后执行命令, 多文件时逐行执行
 * 参考 RunAny: 占位符未带引号且内容含空格时自动包上双引号
 * @param command 命令模板
 * @param content 选中内容
 * @param workingDir 工作目录 (可选)
 */
RunReplaced(command, content, workingDir := "") {
  lines := StrSplit(content, "`n")
  if lines.Length == 1 {
    Run(SelectionContext.Normalize(command, QuoteIfSpace(lines[1])), workingDir)
    return
  }
  for line in lines {
    Run(SelectionContext.Normalize(command, QuoteIfSpace(line)), workingDir)
  }
}

/**
 * 内容含空格且未加引号时自动包上双引号 (参考 RunAny)
 * @param text 文本
 * @returns {string}
 */
QuoteIfSpace(text) {
  q := Chr(34)  ; 字面双引号: AHK v2 中 """" 会解析为两个空字符串, 必须用 Chr(34)
  if InStr(text, " ") and not (SubStr(text, 1, 1) == q) {
    return q text q
  }
  return text
}

/**
 * 执行 AHK 脚本片段: 把 %selected% 替换为字符串字面量, 写入临时脚本用 AutoHotkey 执行
 * @param code 脚本模板
 * @param content 选中内容
 */
RunScriptWithSelected(code, content) {
  file := A_WorkingDir "\data\selected_action_cache.ahk"
  script := StrReplace(code, "%selected%", ToAHKString(content))
  f := FileOpen(file, "w", "UTF-8")
  f.Write(script)
  f.Close()
  Run('"' A_WorkingDir '\bin\AutoHotkey64.exe" "' file '"', A_WorkingDir "\data")
}

/**
 * 转义为 AHK 双引号字符串字面量
 * 注意: 反引号+引号 (`` `" ``) 在字符串内是合法的转义, 但为避免歧义这里用 Chr(34) 构造双引号
 * @param text 文本
 * @returns {string}
 */
ToAHKString(text) {
  ; 反引号 → `` (双反引号)
  text := StrReplace(text, "``", "````")
  ; 双引号 → `" (反引号 + 双引号)
  text := StrReplace(text, Chr(34), "``" Chr(34))
  return Chr(34) text Chr(34)
}

