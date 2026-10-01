; ============================================================
; HistoryStore.ahk —— QuickSwitch 内三层之「唯一磁盘」层。
;
; 唯一职责: 历史 TSV 的读写/裁剪/清空/排除过滤。
; 落盘格式: <路径><TAB><次数><TAB><最后访问(本地 Unix 秒)>, 每行一条, LF 结尾。
; 编码铁律: 所有写入显式 "UTF-8-RAW" (无 BOM 的 UTF-8; AHK FileAppend 默认按系统 ANSI/GBK,
; 见阶段 0 §3.6b / §11 陷阱 22; 而 "UTF-8" 选项会写 BOM, 故取 "-RAW" 变体满足设计 §3.1 的无 BOM 要求)。
; 安全: 含控制字符(CR/LF/TAB/NUL)的路径一律拒绝, 保证 TSV 结构不被注入破坏。
; 依赖方向: 本层不依赖任何其他 quickswitch 模块。
; ============================================================
#Warn All, Off

; 最近一次 HistLoad 因损坏而跳过的行数 (可观测计数, 供上层 / 设置页展示)。
class HistoryStoreDiag {
  static LastBadLines := 0
  static LastLoaded := 0
}

; 读取历史文件为 Map(path -> {count, lastAccess})。
; 文件不存在 -> 空 Map; 单行损坏(列数不符/计数非整数/路径含控制字符) -> 跳过并计数, 不丢弃整份。
HistLoad(filePath) {
  store := Map()
  HistoryStoreDiag.LastBadLines := 0
  HistoryStoreDiag.LastLoaded := 0
  if (filePath = "" || !FileExist(filePath)) {
    return store
  }
  text := ""
  try {
    text := FileRead(filePath, "UTF-8")
  } catch {
    return store
  }
  bad := 0
  for line in StrSplit(text, "`n", "`r") {
    if (Trim(line) = "") {
      continue
    }
    parts := StrSplit(line, "`t")
    if (parts.Length != 3) {
      bad += 1
      continue
    }
    p := parts[1]
    if (p = "" || HistHasControlChar(p)) {
      bad += 1
      continue
    }
    c := HistToInt(parts[2])
    t := HistToInt(parts[3])
    if (c = "" || t = "") {
      bad += 1
      continue
    }
    store[p] := {count: c, lastAccess: t}
  }
  HistoryStoreDiag.LastBadLines := bad
  HistoryStoreDiag.LastLoaded := store.Count
  return store
}

; 记录一次访问: 计数 +1, 刷新最后访问时间。原地修改 store; 非法路径直接忽略。
HistRecord(store, path, ts) {
  if (store = "" || path = "") {
    return
  }
  if (HistHasControlChar(path)) {
    return
  }
  if (store.Has(path)) {
    e := store[path]
    e.count += 1
    e.lastAccess := ts
  } else {
    store[path] := {count: 1, lastAccess: ts}
  }
}

; 落盘: 目录不存在自动创建; 超过 maxEntries 时按最低 lastAccess 裁剪;
; 全量重写(先删后写)保证无残留。返回是否成功。
HistSave(filePath, store, maxEntries) {
  if (filePath = "") {
    return false
  }
  try {
    dir := ""
    SplitPath(filePath, , &dir)
    if (dir != "" && !DirExist(dir)) {
      FileCreateDir(dir)
    }

    arr := []
    for p, e in store {
      if (HistHasControlChar(p)) {
        continue
      }
      count := e.HasProp("count") ? e.count : 0
      lastAccess := e.HasProp("lastAccess") ? e.lastAccess : 0
      arr.Push({path: p, count: count, lastAccess: lastAccess})
    }
    HistSortByLastAccessDesc(arr)

    limit := (maxEntries > 0) ? maxEntries : arr.Length
    out := ""
    i := 1
    for item in arr {
      if (i > limit) {
        break
      }
      out .= item.path "`t" item.count "`t" item.lastAccess "`n"
      i += 1
    }

    if (FileExist(filePath)) {
      FileDelete(filePath)
    }
    FileAppend(out, filePath, "UTF-8-RAW")
    return true
  } catch {
    return false
  }
}

; 清空历史: 截断为空文件 (保留文件本身, 避免后续写入需重建目录)。返回是否成功。
HistClear(filePath) {
  if (filePath = "") {
    return false
  }
  try {
    if (FileExist(filePath)) {
      FileDelete(filePath)
    }
    dir := ""
    SplitPath(filePath, , &dir)
    if (dir != "" && !DirExist(dir)) {
      FileCreateDir(dir)
    }
    FileAppend("", filePath, "UTF-8-RAW")
    return true
  } catch {
    return false
  }
}

; 按前缀排除过滤 (不删数据, 仅返回过滤后的新 Map)。
HistFilterExcluded(store, prefixes) {
  out := Map()
  if (store = "") {
    return out
  }
  for p, e in store {
    if (HistIsExcluded(p, prefixes)) {
      continue
    }
    out[p] := e
  }
  return out
}

; 当前本地 Unix 秒 (持久化口径; 与 FolderRanker 相对时间显示一致)。
HistNowTs() {
  return DateDiff(A_Now, "19700101000000", "Seconds")
}

; ---- 内部工具 ----

; 路径是否命中任一排除前缀 (大小写不敏感, 与 Windows 路径语义一致)。
HistIsExcluded(path, prefixes) {
  if (path = "" || prefixes = "") {
    return false
  }
  for pre in prefixes {
    pre := Trim(pre)
    if (pre = "") {
      continue
    }
    if (SubStr(path, 1, StrLen(pre)) = pre) {
      return true
    }
  }
  return false
}

; 是否含控制字符 (码点 < 32, 含 CR/LF/TAB/NUL); 其余 (含中文) 放行。
HistHasControlChar(s) {
  if (s = "") {
    return false
  }
  return RegExMatch(s, "[\x00-\x1F]") != 0
}

; 严格整数解析: 非整数返回 ""。
HistToInt(s) {
  s := Trim(s)
  if (IsInteger(s)) {
    return Integer(s)
  }
  return ""
}

; lastAccess 降序稳定排序 (裁剪时保留"最近"的 maxEntries 条)。
HistSortByLastAccessDesc(arr) {
  n := arr.Length
  i := 2
  while (i <= n) {
    key := arr[i]
    j := i - 1
    while (j >= 1 && arr[j].lastAccess < key.lastAccess) {
      arr[j + 1] := arr[j]
      j -= 1
    }
    arr[j + 1] := key
    i += 1
  }
  return arr
}
