; ============================================================
; EverythingProviders —— Everything 查询通道抽象。
;
; 为什么把「通道」抽象出来: Everything 有 4 条可编程通道, 可用性/可移植性差异很大
; (实测与官方文档见 README「通道选型」):
;   1) ES 命令行 (es.exe)        <- 本插件首选。官方 CLI, 自带 -ipc1/-ipc2/-ipc3 自适应
;                                   Everything 1.4/1.5, 结果可 -export-txt 落文件, 退出码有语义。
;   2) WM_COPYDATA IPC           <- 不用。实测 Everything 1.5.0.1418 回复不稳定, 且回复数据在
;                                   对端进程地址空间 (需 ReadProcessMemory), 发送线程阻塞时无法
;                                   处理回复 (需另开线程), AHK 单线程模型下不可靠。
;   3) Everything64.dll (SDK2)   <- 不用。需随插件分发 dll, 且官方文档明确 1.5 alpha 分支不支持
;                                   (Flow Launcher 即此路线, 其文档标注「仅支持 1.4.x」)。
;   4) everything.exe -search    <- 降级。仅把 Everything 界面打开到搜索结果, 结果不在本插件
;                                   下拉列表里 —— 只在 1) 缺失且 2)3) 不可用时使用, 并明确告知用户。
;
; 统一结果契约: {ok:Boolean, error:String, items:Array<{path, name, isFolder}>}
; ============================================================

; 错误码 (跨文件共享; AHK 全局常量)
global ES_ERR_NOT_FOUND := "es-not-found"         ; es.exe 不存在/无法启动
global ES_ERR_NOT_RUNNING := "everything-down"    ; Everything 未运行
global ES_ERR_EXPORT := "export-failed"           ; es.exe 无法写导出文件
global ES_ERR_QUERY := "query-failed"             ; 其他查询失败
global ES_ERR_NO_ES_LAUNCHED := "gui-launched"    ; 已降级为打开 Everything 界面
global ES_ERR_LAUNCH_FAILED := "launch-failed"    ; 降级也失败
global ES_ERR_EMPTY := "empty-query"
global ES_ERR_NO_PATH := "no-everything-path"     ; 未配置 everything.exe 路径

/** 查询通道基类 (接口契约, 见文件头)。 */
class EverythingProvider {
  Name := "base"

  /**
   * @param query 检索词 (已归一)
   * @param limit 结果条数上限
   * @returns {{ok:Boolean, error:String, items:Array}}
   */
  Search(query, limit) {
    return {ok: false, error: ES_ERR_QUERY, items: []}
  }
}

/**
 * ES 通道 —— 官方命令行 es.exe。
 * 调用形态: es.exe -timeout 4000 -n <limit> -export-txt "<out>" "<query>"
 *   -timeout   : 等 Everything 数据库加载完成 (冷启动/刚拉起时必需, 否则静默返回空)
 *   -n         : 结果条数上限
 *   -export-txt: 结果按行落文件 (AHK 无法直接读子进程 stdout, 这是最稳的一条; 官方选项)
 * 退出码 (官方文档): 0 成功 / 5 无法创建导出文件 / 7 IPC 查询失败 / 8 找不到 IPC 窗口
 * (即 Everything 未运行)。
 */
class EverythingEsProvider extends EverythingProvider {
  Name := "es-cli"

  ; 临时导出目录 (测试可注入; 运行期默认系统临时目录)
  static TempDir := ""

  __New(exe) {
    this.exe := exe
  }

  Search(query, limit) {
    out := this._OutFile()
    cmd := '"' this.exe '" -timeout 4000 -n ' limit this._SortArgs() ' -export-txt "' out '"' this._QuerySuffix(query)
    code := 0
    try {
      code := RunWait(cmd, , "Hide")
    } catch {
      ; RunWait 抛错 = 可执行文件不存在 (含 PATH 里没有的裸名 es.exe)
      return {ok: false, error: ES_ERR_NOT_FOUND, items: []}
    }
    if (code = 8)
      return {ok: false, error: ES_ERR_NOT_RUNNING, items: []}
    if (code = 5)
      return {ok: false, error: ES_ERR_EXPORT, items: []}
    if (code != 0)
      return {ok: false, error: ES_ERR_QUERY, items: []}

    text := ""
    try {
      if FileExist(out)
        text := FileRead(out, "UTF-8")
    } catch {
      return {ok: false, error: ES_ERR_EXPORT, items: []}
    }
    try FileDelete(out)   ; 结果已读入内存; 删除失败不影响功能 (临时目录可回收)
    return {ok: true, error: "", items: this.ParseExport(text, limit)}
  }

  /**
   * 解析 -export-txt 产物: 每行一个完整路径。
   * 文件夹判定: es 默认给文件夹路径追加尾部分隔符 (官方 -no-folder-append-path-separator 的反证),
   * 同时用 FileExist 复核 (路径可能已不存在而目录标志仍在)。
   * @returns {Array<{path, name, isFolder}>}
   */
  ParseExport(text, limit) {
    items := []
    if (text = "")
      return items
    text := StrReplace(text, "`r`n", "`n")
    text := StrReplace(text, "`r", "`n")
    for line in StrSplit(text, "`n") {
      p := Trim(line, " `t")
      if (p = "")
        continue
      isFolder := (SubStr(p, -1) = "\") || (FileExist(p) = "D")
      if (isFolder)
        p := RTrim(p, "\")
      if (p = "")
        continue
      items.Push({path: p, name: this._BaseName(p), isFolder: isFolder})
      if (items.Length >= limit)
        break
    }
    return items
  }

  ; ---- 内部 ----

  ; ---- 排序对齐 (2026-09-25 用户报障: 命令框结果与 Everything GUI 不一致) ----
  ; GUI 的结果列表按其当前排序展示 (本机实测 Everything.ini: sort=Date Modified +
  ; sort_ascending=0 = 最近修改优先), 而 es.exe 不传 -sort 时按名称升序返回 —— 同一
  ; 查询两组文件的前 N 条完全不同 (-n 截断放大差异)。故读 ini 的 sort=/sort_ascending=
  ; 映射成 es 的 -sort 参数, 让下拉列表与 GUI 同序。
  ;
  ; 🔴 实现纪律 (v1.0.1 首版事故教训): 第一版用「类静态 Map 初始化器」做查表, 在引擎
  ;    AHK 运行时里静态属性未实例化 → 每次查询抛 PropertyError 被会话吞掉 → 无任何
  ;    搜索结果 + 命令框退回命令匹配 (引擎日志 command_input_hooks.log 实录)。故本版:
  ;    ① 不引入任何**加载期求值**的代码 (无类静态初始化器, 白名单改为方法内 switch);
  ;    ② 整体 try/catch —— 任何意外 (编码/权限/解析) 都退回 es 默认排序, 绝不让查询
  ;    路径抛异常。

  /**
   * 组装 -sort 参数 (镜像 GUI 当前排序):
   * ini 位置: 便携版在 everything.exe 旁 (本机 app_data=0); 安装版在 %APPDATA%\Everything。
   * 方向: sort_ascending=0 → -sort-descending (本机 Date Modified 即此档 = 最近修改优先),
   *       否则 -sort-ascending; ini/键缺失 → 返回 "" (es 默认名称升序)。
   * 每次查询重读 ini: 用户在 GUI 点列头改排序后无需重启, 下一次查询即跟随。
   */
  _SortArgs() {
    try {
      ini := this._EverythingIni()
      if (ini = "")
        return ""
      text := FileRead(ini, "UTF-8")
      if (!RegExMatch(text, "m)^sort=(.*)$", &m))
        return ""
      sortKey := StrLower(Trim(m[1]))
      es := ""
      switch sortKey {
        case "name": es := "name"
        case "path": es := "path"
        case "size": es := "size"
        case "date modified": es := "date-modified"
        case "date created": es := "date-created"
        case "extension": es := "extension"
        case "date recently changed": es := "date-recently-changed"
        case "run count": es := "run-count"
        case "date run": es := "date-run"
        case "attributes": es := "attributes"
        case "file list filename": es := "file-list-filename"
        case "availability": es := "availability"
        case "type": es := "type"
      }
      if (es = "")
        return ""
      asc := RegExMatch(text, "m)^sort_ascending=(.*)$", &a) ? Trim(a[1]) : "1"
      return " -sort " es ((asc = "0") ? " -sort-descending" : " -sort-ascending")
    } catch {
      return ""  ; 任何意外 (编码/权限/解析) 都不破坏查询 —— 退回 es 默认排序
    }
  }

  /** Everything.ini 落点: 便携版 (app_data=0) 在 everything.exe 旁; 否则 %APPDATA%\Everything。 */
  _EverythingIni() {
    exe := EverythingSettings.EverythingPath
    if (exe != "") {
      SplitPath(exe, , &dir)
      cand := dir "\Everything.ini"
      if FileExist(cand)
        return cand
    }
    cand := A_AppData "\Everything\Everything.ini"
    return FileExist(cand) ? cand : ""
  }

  _OutFile() {
    ; 注意: TempDir 是静态属性, 实例方法里必须经类名访问 (AHK 静态属性不在实例上)
    dir := (EverythingEsProvider.TempDir != "") ? EverythingEsProvider.TempDir : A_Temp
    return dir "\kf_es_" A_ScriptHwnd "_" A_TickCount ".txt"
  }

  /**
   * 检索词归一为命令行参数:
   *   - 折叠空白 (Everything 用空格分隔 AND 词项, 换行会让解析歧义);
   *   - 去掉双引号 (AHK 的 Run 无法表达嵌套引号, 而引号在 Everything 语法里只是短语包裹);
   *   - 以 "-" 开头时前置 "--" (官方「禁用开关解析」约定), 避免检索词被当开关。
   */
  _QuerySuffix(query) {
    q := Trim(RegExReplace(query, "\s+", " "), " ")
    q := StrReplace(q, '"', " ")
    if (q = "")
      return ""
    if (SubStr(q, 1, 1) = "-")
      return ' -- "' q '"'
    return ' "' q '"'
  }

  _BaseName(p) {
    i := InStr(p, "\", , -1)
    return (i > 0) ? SubStr(p, i + 1) : p
  }
}

/**
 * 降级通道 —— 用 everything.exe 直接把界面打开到搜索结果。
 * 触发条件: es.exe 缺失 (用户未装 CLI)。结果不在本插件下拉列表里, 浮层会明确说明。
 */
class EverythingGuiProvider extends EverythingProvider {
  Name := "gui-launch"

  __New(exe) {
    this.exe := exe
  }

  Search(query, limit) {
    if (this.exe = "" || !FileExist(this.exe))
      return {ok: false, error: ES_ERR_NOT_FOUND, items: []}
    q := Trim(RegExReplace(query, "\s+", " "), " ")
    q := StrReplace(q, '"', " ")
    try {
      Run('"' this.exe '" -search "' q '"')
    } catch {
      return {ok: false, error: ES_ERR_LAUNCH_FAILED, items: []}
    }
    return {ok: false, error: ES_ERR_NO_ES_LAUNCHED, items: []}
  }
}

/** 通道探测: 决定用哪条通道 (纯函数 + 一次缓存, 便于单测)。 */
class EverythingProviders {
  static ResolvedEs := ""
  static ResolvedDone := false

  /**
   * 按优先级探测 es.exe 落点 (结果缓存, 只在首次查询时探测一次):
   *   1) 设置里的显式 esPath
   *   2) everything.exe 同目录 (最常见: 解压版同目录 / 安装版同目录)
   *   3) 插件自带 bin/es.exe
   *   4) 系统 PATH (用 `es.exe -version` 实测一次, 避免把「不存在」拖到查询期)
   * @returns {String} "" = 完全不可用 (调用方改走降级通道)
   */
  static ResolveEs() {
    if (this.ResolvedDone)
      return this.ResolvedEs
    this.ResolvedDone := true
    this.ResolvedEs := ""

    if (EverythingSettings.EsPath != "" && FileExist(EverythingSettings.EsPath)) {
      this.ResolvedEs := EverythingSettings.EsPath
      return this.ResolvedEs
    }

    exe := EverythingSettings.EverythingPath
    if (exe != "" && FileExist(exe)) {
      SplitPath(exe, , &dir)
      cand := dir "\es.exe"
      if FileExist(cand) {
        this.ResolvedEs := cand
        return this.ResolvedEs
      }
    }

    self := EverythingSettings.SelfEsPath
    if (self != "" && FileExist(self)) {
      this.ResolvedEs := self
      return this.ResolvedEs
    }

    ; PATH 探测: -version 立即退出, 失败则不是可用 CLI
    try {
      if (RunWait('es.exe -version', , "Hide") = 0)
        this.ResolvedEs := "es.exe"
    } catch {
      this.ResolvedEs := ""
    }
    return this.ResolvedEs
  }

  /** 探测结果失效 (设置变更后调用, 下次查询重新探测)。 */
  static Reset() {
    this.ResolvedDone := false
    this.ResolvedEs := ""
  }

  /** 构造本次查询要用的通道实例。 */
  static Create() {
    es := this.ResolveEs()
    if (es != "")
      return EverythingEsProvider(es)
    return EverythingGuiProvider(EverythingSettings.EverythingPath)
  }
}
