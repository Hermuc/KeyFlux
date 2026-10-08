; ------------------------------------------------------------
; 模块布局 (2026-10-08 拆分; 模块化审查 §3.3 / 问题 #4)
;
; 本文件曾是 1037 行的「总装车间」, 一个文件混装 6 个职责。现为**门面**:
;   SelectedAction.ahk                 本文件 —— 契约头注 + 子模块清单 + 两个顶层入口
;   SelectedAction/Dispatch.ahk        class SelectedAction 第一段: 静态状态 + Trigger + 行序匹配
;   SelectedAction/Menu.ahk            class 第二段: 打开菜单 / 序号选择 / 链式按键 / 取消与淡出
;   SelectedAction/Execute.ahk         class 第三段: 8 列 entry 的实际执行
;   SelectedAction/Sample.ahk          class 第四段: ▶ 示例播放 + 请求文件监视
;   SelectedAction/Matching.ahk        匹配原语 (内置文本特征 / 自定义类型 / 后缀分组 / ASCII 折叠)
;   SelectedAction/Selection.ahk       选区与路径操作
;   SelectedAction/Handlers.ahk        内容处理器 (磁力链 / 注册表 / 命令替换 / 脚本)
;
; 🔴 **生成端一行都不用改**: templates/keyflux.tmpl 里仍是
;    `#Include lib/rules/SelectedAction.ahk`。子文件经**嵌套 #Include** 引入
;    (AHK v2 允许在顶层与 class 体内使用, 相对路径按**所在文件目录**解析),
;    故生成的 KeyFlux.ahk **字节不变** ⇒ parity `ahk` 基线不受影响。
;    先例: lib/actions/Actions.ahk → builtins\*.ahk 早已如此。
; 🔴 拆 class 靠的是「#Include 可放在 class 体内」—— 见 Dispatch.ahk 头注。
; ------------------------------------------------------------

; ============================================================
; 选中动作系统 (Selected Action) —— 方案 D「单键分发」
; 参考 RunAny 的「选中内容 + 快捷键触发预设行为」能力
;
; 生成端契约 (config-ui-reactor 生成器 selectedActionCode 渲染; 任务 #29 冻结):
;   SelectedActionData := Array(
;     {matchType: "textType", matchValue: "url", key: 1, behavior: "open_url",
;      action: "open_url", actionValue: "", workingDir: "", name: "open_url"},
;     ...)
;   SelectedActionInit(">^p", SelectedActionData)
;
; 8 列含义: matchType (textType=内置文本特征值, 见 TextFeatureSpecs; fileExt=逗号分隔后缀) /
;   matchValue (条件值) / key (菜单序号 1-9, 同一 mapping 内从 1 递增) /
;   behavior (行为库 ID) / action (ResolveRuleAction 展开后基础动作) /
;   actionValue (展开后模板) / workingDir (工作目录) / name (显示名)。
; 数组顺序 = mappings 配置顺序 = 匹配优先级; 连续同 (matchType, matchValue) 的行
; 构成同一匹配组 (同一 mapping 的展开), key 在组内即菜单序号。
;
; 执行模型 (定稿): 选中文本/文件 -> 按主快捷键 -> 按数组行序找到首个类型匹配的组:
;   - 组内仅 1 条 entry: 直接执行;
;   - 组内多条: 弹无焦点菜单 (InputTipWindow 同款小窗), 按数字 1-9 立即执行对应项,
;     Esc 或重复按主键取消, 5s 超时自动取消, 淡入淡出不抢焦点;
;   - 无任何组匹配: Tip 气泡提示「未识别的类型」。
;
; 决策记录 (2026-09-05): 本版引擎不做 confirm/copyToClipboard/clearSelection 选项 ——
; 方案 D 菜单语义下这三个选项的行为未定义, 生成端 (任务 #29) 也未把 entry.options
; 写进数据数组; 执行模型定稿只有: 直接执行 / 菜单 / Esc / 超时。
;
; 阶段说明: 本文件重写入口与分发层 (SelectedActionInit / SelectedAction 类);
; 匹配原语 (MatchTextType/MatchFileExt) 与执行辅助 (OpenSelectedPaths/OpenSelectedFolder/
; DownloadMagnet/OpenRegistryKey/RunReplaced/RunScriptWithSelected 等) 原样保留;
; 选中内容获取统一在 SelectionContext, 执行层未来委托 ActionRegistry。
; 变量约定: {selected} 为规范形, %selected% 为兼容形 (由 context/SelectionContext.ahk
; 归一处理); 多文件用换行分隔, 与资源管理器复制文件到剪贴板的格式一致。
; ============================================================
#Include SelectedAction\Dispatch.ahk    ; class SelectedAction（内部再 include Menu/Execute/Sample）
#Include SelectedAction\Matching.ahk    ; 匹配原语
#Include SelectedAction\Selection.ahk   ; 选区与路径操作
#Include SelectedAction\Handlers.ahk    ; 内容处理器

/**
 * 初始化选中动作: 为「单键分发」注册主快捷键 (生成端在 InitKeymap 中调用)
 * @param hotkey 主快捷键 (如 ">^p"); 禁用/空热键时生成端输出空串不调用本函数, 此处兜底跳过
 * @param entries 数据数组 (契约见文件头), 每项 8 列
 */
SelectedActionInit(hotkeyName, entries) {
  if (hotkeyName == "" || !IsObject(entries) || entries.Length == 0) {
    return
  }
  ; 彩蛋 (▶ 真实执行): 留存 entries 副本 + 注册 250ms 轮询 (文件不存在时零开销返回)
  SelectedAction.Data := entries
  if not (SelectedAction.PlayTimerStarted) {
    SelectedAction.PlayTimerStarted := true
    ; ⚠ AHK v2 不接受「静态方法引用」直接作 SetTimer 回调 (ValueError: Invalid callback
    ; function), 须传可调用的函数对象 —— 用闭包包一层 (与 bin/lib 既有 SetTimer(() => ...)
    ; 惯例一致; 2026-09-23 实测引擎加载即报此错, 已修复)。
    SetTimer(() => SelectedAction.WatchPlayRequest(), 250)
  }
  ; N 键链式热键 (物理键数 ≥3): AHK 原生自定义组合只支持两键, 拆为「头部热键 + 尾部 InputHook 顺序匹配」。
  ; 头部两种形态 (UI 生成端 HotkeyCaptureCore.CommitStaged 对应):
  ;   纯键链   "j & k & l"   -> 头部注册自定义组合 "j & k", 尾部 ["l"]
  ;   修饰链   "<^j & k & l" -> 头部注册普通修饰热键 "<^j", 尾部 ["k","l"]
  ; 修饰链不能用 "LCtrl & j" 自定义组合形式: 组合不能混修饰键, 且那样会把 LCtrl
  ; 全局注册为前缀键, 改变系统里单独按 LCtrl 的行为 (副作用), 故头段走普通热键。
  parts := StrSplit(hotkeyName, "&", " ")
  head := Trim(parts[1])
  headIsMod := parts.Length >= 2 && HotkeyHeadHasModifier(head)
  if (parts.Length >= 3 || headIsMod) {
    if (headIsMod) {
      remaining := []
      Loop parts.Length - 1 {
        remaining.Push(Trim(parts[A_Index + 1]))
      }
    } else {
      head := head " & " Trim(parts[2])
      remaining := []
      Loop parts.Length - 2 {
        remaining.Push(Trim(parts[A_Index + 2]))
      }
    }
    chain(thisHotkey) {
      SelectedAction._ChainWait(head, remaining, hotkeyName, entries)
    }
    try {
      KeymapManager.GlobalKeymap.Map(head, chain, , , , "S")
    } catch {
      return
    }
    return
  }
  trigger(thisHotkey) {
    SelectedAction.Trigger(hotkeyName, entries)
  }
  ; 无效热键(如反引号)注册失败时跳过该方案, 避免单个方案拖垮整个脚本 (与旧 InitActionScheme 同策略)
  try {
    KeymapManager.GlobalKeymap.Map(hotkeyName, trigger, , , , "S")
  } catch {
    return
  }
}

/**
 * 头段是否为「带修饰键的普通热键」(如 "<^j"): 含 AHK 修饰符字符即视为是。
 * 链式 UI 生成端只产生侧别前缀 (<^ >^ <! >! <+ >+ <# >#) 形式, 纯键名不含这些字符。
 */
HotkeyHeadHasModifier(part) {
  return InStr(part, "^") || InStr(part, "!") || InStr(part, "+") || InStr(part, "#")
}

