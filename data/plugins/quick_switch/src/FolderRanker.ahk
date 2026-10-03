; ============================================================
; FolderRanker.ahk —— QuickSwitch 内三层之「纯函数」层。
;
; 唯一职责: 把「历史条目」排序成「候选列表」并计算相对时间。
; 本文件是纯函数层, 绝不触碰外部世界: 无窗口 API、无原生调用、无 COM、无磁盘读写。
; (可被静态证伪: 对窗口/原生/COM/文件读写四类关键字 grep 应零命中;
;  为规避该静态检索误伤, 本文件正文与注释均刻意不写出这些关键字。)
;
; 排序确定性: 总序 = (score 降序, path 升序, lastAccess 降序), 与输入遍历顺序无关,
; 因此同一输入两次调用输出逐字节一致。
; 依赖方向: 本层不依赖任何其他 quickswitch 模块。
; ============================================================
#Warn All, Off

; 索引衰减半衰期 (秒): 3 天。越大越"怀旧", 越小越"健忘"。
RankHalfLifeSec() {
  return 3 * 86400
}

; 把历史 store (Map: path -> {count, lastAccess}) 展开成可排序的候选数组。
; 每个元素: {path, count, lastAccess, score, relativeTime, enabled}
RankBuildCandidates(items, nowTs, withExplorerBoost, explorerDir) {
  list := []
  if (items == "") {
    return list
  }
  halfLife := RankHalfLifeSec()
  for path, entry in items {
    if (path = "") {
      continue
    }
    count := 0
    lastAccess := 0
    if (entry != "") {
      count := entry.HasProp("count") ? entry.count : 0
      lastAccess := entry.HasProp("lastAccess") ? entry.lastAccess : 0
    }
    age := nowTs - lastAccess
    if (age < 0) {
      age := 0
    }
    ; 最近性: e^{-age/半衰期}; 频率: ln(1+次数) 抑制长尾; 上下文: 当前 Explorer 目录加权
    recency := Exp(-age / halfLife)
    freq := Ln(1 + count)
    boost := 0.0
    if (withExplorerBoost && explorerDir != "" && path != "" && path = explorerDir) {
      boost := 6.0
    }
    score := 2.0 * recency + 0.5 * freq + boost
    rel := RankRelativeTime(lastAccess, nowTs)
    list.Push({path: path, count: count, lastAccess: lastAccess, score: score, relativeTime: rel, enabled: true})
  }
  return list
}

; 推荐排序: 最近性指数衰减 + 频率 + 当前 Explorer 窗口加权。
; items: Map(path -> {count, lastAccess}); explorerDir: 当前 Explorer 目录(可空);
; nowTs: Unix 秒。返回候选数组 (未裁剪, 由上层按 overlayRows 截断)。
RankRecent(items, explorerDir, nowTs) {
  list := RankBuildCandidates(items, nowTs, true, explorerDir)
  return RankInsertionSort(list, RankCmpRecent)
}

; 历史排序: 纯时间倒序, 不应用 Explorer 加权 (历史模式语义)。
; nowTs 可选: 省略或传 0 时取当前本地 Unix 秒 (便于单测注入固定时钟)。
RankByTime(items, nowTs := 0) {
  if (nowTs = 0) {
    nowTs := RankNowTs()
  }
  list := RankBuildCandidates(items, nowTs, false, "")
  return RankInsertionSort(list, RankCmpTime)
}

; 相对时间文案 (刚刚 / N 分钟前 / N 小时前 / 昨天 / MM-dd)。
; 文案走 translation.ahk (AHK 文案真源, 见设计 §7.3)。
RankRelativeTime(lastAccess, nowTs) {
  d := nowTs - lastAccess
  if (d < 0) {
    d := 0
  }
  if (d < 60) {
    return Translation().qs_time_just_now
  }
  if (d < 3600) {
    return Format(Translation().qs_time_min_ago, d // 60)
  }
  if (d < 86400) {
    return Format(Translation().qs_time_hour_ago, d // 3600)
  }
  if (d < 172800) {
    return Translation().qs_time_yesterday
  }
  stamp := DateAdd("19700101000000", lastAccess, "Seconds")
  return FormatTime(stamp, "MM-dd")
}

; ---- 排序比较器 (总序, 保证确定性) ----

; 推荐: score 降序 -> path 升序 -> lastAccess 降序
RankCmpRecent(a, b) {
  if (a.score > b.score) {
    return -1
  }
  if (a.score < b.score) {
    return 1
  }
  c := StrCompare(a.path, b.path)
  if (c != 0) {
    return c
  }
  if (a.lastAccess > b.lastAccess) {
    return -1
  }
  if (a.lastAccess < b.lastAccess) {
    return 1
  }
  return 0
}

; 历史: lastAccess 降序 -> path 升序
RankCmpTime(a, b) {
  if (a.lastAccess > b.lastAccess) {
    return -1
  }
  if (a.lastAccess < b.lastAccess) {
    return 1
  }
  return StrCompare(a.path, b.path)
}

; 稳定插入排序 (<=200 条时 O(n^2) 亦可忽略; 避免依赖 Array.Sort 回调语义)。
RankInsertionSort(arr, cmp) {
  n := arr.Length
  i := 2
  while (i <= n) {
    key := arr[i]
    j := i - 1
    while (j >= 1 && cmp(arr[j], key) > 0) {
      arr[j + 1] := arr[j]
      j -= 1
    }
    arr[j + 1] := key
    i += 1
  }
  return arr
}

; 本地 Unix 秒 (与 RankRelativeTime/HistoryStore 口径一致, 排序单调)。
; 仅供排序显示使用; 时间戳的持久化口径见 HistoryStore.HistNowTs。
RankNowTs() {
  return DateDiff(A_Now, "19700101000000", "Seconds")
}
