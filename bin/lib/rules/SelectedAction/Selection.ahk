; ============================================================
; SelectedAction —— 选区与路径操作（顶层函数，非 class 成员）
;
; 由门面 SelectedAction.ahk 在**顶层** #Include。取选中内容 + 打开选中路径/所在文件夹。
; ============================================================


/**
 * 获取当前选中内容 (实现已迁移到 context/SelectionContext.ahk, 保留函数签名兼容存量调用)
 * @deprecated 新代码请直接用 SelectionContext.Get(); 本壳保留是为兼容用户自定义代码
 *             (data/custom_functions.ahk) 与旧配置中的遗留引用, 勿删除。
 * @returns {{type: string, content: string}} type: file / text / ""
 */
GetSelectedContent() {
  return SelectionContext.Get()
}


/**
 * 打开选中路径 (逐行), 按系统关联程序打开, 等同资源管理器双击
 * @param content 路径列表 (换行分隔)
 */
OpenSelectedPaths(content) {
  for line in StrSplit(content, "`n") {
    line := Trim(line)
    if (line) {
      Run(QuoteIfSpace(line))
    }
  }
}

/**
 * 打开选中路径所在文件夹: 选中本身是目录时直接打开, 是文件时打开其父目录
 * 多选时只处理第一行 (行为语义: 打开第一个路径所在文件夹)
 * @param content 选中内容
 */
OpenSelectedFolder(content) {
  line := Trim(StrSplit(content, "`n")[1])
  if not (line) {
    return
  }
  ; FileExist 返回属性串, 含 "D" 表示目录
  if InStr(FileExist(line), "D") {
    Run(QuoteIfSpace(line))
    return
  }
  SplitPath(line, , &dir)
  if (dir) {
    Run(QuoteIfSpace(dir))
  }
}
