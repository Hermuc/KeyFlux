; ============================================================
; SelectedAction —— 示例播放与请求监视（class SelectedAction 第四段）
;
; 由 Dispatch.ahk 在 class 体内 #Include。设置界面 ▶ 的请求文件轮询 + 示例播放。
; ============================================================

  /**
   * 彩蛋 (▶ 真实执行): 按 typeId 用该类型真实配置的行为直接执行预设样例。
   * typeId 来自设置界面 ▶ 经后端白名单校验后写入的请求文件 (见 WatchPlayRequest):
   *   - 内置文本特征 (url/path/magnet/bilibili/plain): 命中 textType 组, 样例为文本;
   *   - "type:<id>" (自定义类型): 查 CustomMatchTypes 决定 text/file, 命中对应引用组;
   *   - 文件后缀组 (已折叠为规范化后缀串, 如 "jpg,png"): 命中 fileExt 组, 样例为文件 (A_Desktop)。
   * 样例内容硬编码 (不进配置), 与 _Execute 真实执行同一条入口; 未命中/未配置用现有翻译文案提示
   * (不新增 i18n 键)。多行为时执行组内首条 (与菜单序号 1 等价); 不改菜单逻辑。
   * @param typeId 解析后的条件值: 内置特征值 / "type:<id>" / 规范化后缀串 (group 已由后端折叠)
   */
  static PlaySample(typeId) {
    if not (IsObject(this.Data) && this.Data.Length > 0) {
      Tip(Translation().no_matching_type, -1500)   ; 未配置选中动作
      return
    }
    ; 决定样例形态: 文本 → type:"text"; 其余 (文件后缀/自定义 fileExt) → type:"file"
    isText := false
    if (typeId == "url" || typeId == "path" || typeId == "magnet" || typeId == "bilibili" || typeId == "plain") {
      isText := true
    } else if (SubStr(typeId, 1, 5) == "type:") {
      mt := ResolveMatchValue(typeId)
      isText := !(IsObject(mt) && mt.kind == "fileExt")
    }
    ; 文件后缀组 (group 折叠后的后缀串) 与 type:<id>(fileExt) → 文件样例; 其余 → 文本样例
    if (isText) {
      selected := {type: "text", content: this._SampleText(typeId)}
    } else {
      selected := {type: "file", content: A_Desktop}
    }
    group := this._FindGroup(typeId)
    if (group.Length == 0) {
      Tip(Translation().no_matching_type, -1500)   ; 未命中对应类型
      return
    }
    this._Execute(group[1], selected)   ; 组内首条 (等价菜单序号 1)
  }

  /**
   * 文本特征彩蛋样例内容 (硬编码, 不进配置)。
   * path 用 A_Desktop (真实存在的目录), open_path 会直接打开它; 其余为可识别的样例文本。
   */
  static _SampleText(typeId) {
    switch typeId {
      case "url": return "https://github.com/Hermuc/KeyFlux"
      case "path": return A_Desktop
      case "magnet": return "magnet:?xt=urn:btih:0000000000000000000000000000000000000000"
      case "bilibili": return "BV1xx411c7mD"
      default: return "示例文本 sample text"
    }
  }

  /**
   * 按 matchValue 精确查找匹配组 (复用 _FirstMatch 的连续同值分组逻辑, 但按条件值而非选中内容匹配):
   * 返回首个 matchValue 等于 typeId 的组 (数组); 无匹配返回空数组。
   * 内置文本特征 (matchValue=特征值) / type:<id> (matchValue=引用串) / 后缀组 (matchValue=后缀串)
   * 均按条件值唯一命中, 不依赖选中内容, 故文件后缀组即使用目录样例也能稳定定位。
   */
  static _FindGroup(matchValue) {
    data := this.Data
    n := data.Length
    i := 1
    while (i <= n) {
      mt := data[i].matchType
      mv := data[i].matchValue
      j := i
      while (j < n && data[j + 1].matchType == mt && data[j + 1].matchValue == mv) {
        j++
      }
      if (mv == matchValue) {
        group := Array()
        Loop j - i + 1 {
          group.Push(data[i + A_Index - 1])
        }
        return group
      }
      i := j + 1
    }
    return Array()
  }

  /**
   * 轮询设置界面下发的彩蛋请求文件 (%TEMP%\kf_play_request.json):
   *   - 文件不存在 → 零开销返回 (250ms 一次的常规态);
   *   - 读出 typeId + seq, 校验格式, 执行后**删除文件** (无论成败, 幂等);
   *   - seq 去重: 记录上次执行的 seq, 重复 seq 跳过 (防御文件删除与重读的竞态);
   * 请求文件内容由后端白名单校验后写入, 不含任何命令/路径参数, 无注入面。
   */
  static WatchPlayRequest() {
    path := A_Temp "\kf_play_request.json"
    if not (FileExist(path)) {
      return
    }
    try content := FileRead(path)
    catch as err {
      ; 文件存在却读不出 (后端写入中途被抢读 / 被占用 / 权限) ⇒ 与下方「格式异常」同款处置:
      ; 删文件避免 250ms 一次的 tick 无限重试, 并留痕 (否则静默失败, 彩蛋功能失效能潜伏很久)。
      EngineLogWarn("SelectedAction.WatchPlayRequest: 请求文件读取失败", "path=" path " err=" err.Message)
      FileDelete(path)
      return
    }
    ; 解析 typeId 与 seq (引擎无内置 JSON 库, 用受控格式的正则提取)
    if not (RegExMatch(content, '"typeId"\s*:\s*"([^"]*)"', &m)) {
      FileDelete(path)   ; 格式异常 → 删文件避免反复触发
      return
    }
    typeId := m[1]
    seq := 0
    if (RegExMatch(content, '"seq"\s*:\s*(\d+)', &sm)) {
      seq := Integer(sm[1])
    }
    if (seq == this.PlayLastSeq) {
      return   ; 已执行过该 seq (防御竞态; 常规下 seq 单调递增不会重复)
    }
    this.PlayLastSeq := seq
    FileDelete(path)   ; 执行与否都删, 保证幂等、不残留
    this.PlaySample(typeId)
  }
