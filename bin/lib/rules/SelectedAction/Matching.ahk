; ============================================================
; SelectedAction —— 匹配原语（顶层函数，非 class 成员）
;
; 由门面 SelectedAction.ahk 在**顶层** #Include。内容 = 内置文本特征注册表、
; 自定义匹配类型求值、文件后缀/分组匹配、ASCII 大小写折叠。
; 🔴 texttype 闸门 (devtools texttype-conformance) 逐字抽取本文件的
;    TextFeatureSpecs / TextFeatureHit / MatchTextType —— 改名或迁移须同步该工具。
; ============================================================

; ============================================================
; 自定义匹配类型 (方案 C7): 用户可在设置界面新增"文本特征"(如 网盘链接) 与"文件后缀分组",
; 映射的条件值写 "type:<id>" 引用它们, 生成端把"可用匹配类型"渲染为全局表 CustomMatchTypes
; (config-ui-reactor/src/generator/config.rs 的 CustomMatchTypes 移植; 挂载点
;  templates/keyflux.tmpl:74)。表结构:
;   {id: {kind: "text", rules: [{op, value}, ...]} | {kind: "fileExt", exts: [...]}}
;   - kind=text    -> 4 个封闭算子 OR 求值 (MatchCustomRules)
;   - kind=fileExt -> 后缀集匹配 (MatchFileExtList) —— 自定义文件类型与文件分组共用该形态
; 无自定义类型且无文件分组时生成端不输出该表 (下方 IsSet 守卫兜底)。
;
; 挂载点位于 InitKeymap() 函数体内, 故生成片段自带 global 前缀; 真机实测该 global 赋值对本类
; 的 static 方法可见 (2026-09-15 spike)。契约: 数据数组仍 8 列, 仅 matchValue 值域新增
; "type:<id>" 一类形态 (加法扩展)。
;
; 与 Go 端一致性: 算子语义由双端一致性向量
; (testdata/text_types.json 向量对账) 守护; 大小写折叠一律走 AsciiLower
; (仅折 A-Z), 禁止 StrLower —— 后者对非 ASCII 做区域相关变换, 会造成两端语义分歧。
; ============================================================

/**
 * 是否为自定义匹配类型引用 ("type:<id>")。
 * `:` 是 Windows 文件名非法字符, 故该前缀不可能与任何真实文件后缀或内置特征值碰撞。
 */
IsCustomMatchRef(matchValue) {
  return SubStr(matchValue, 1, 5) == "type:"
}

/**
 * 把引用解析为类型定义对象; 非引用、无表、或未命中一律返回 ""。
 * 调用方据此区分"内置分支"与"引用不命中" (后者一律不匹配, 与 Go 端 c==nil / ok=false 同口径)。
 * @returns {object|string} 表项 (含 kind/rules|exts), 或 ""
 */
ResolveMatchValue(matchValue) {
  global CustomMatchTypes
  if not (IsCustomMatchRef(matchValue)) {
    return ""
  }
  if not (IsSet(CustomMatchTypes)) {
    return ""
  }
  id := SubStr(matchValue, 6)
  if not (CustomMatchTypes.Has(id)) {
    return ""
  }
  return CustomMatchTypes[id]
}

/**
 * 仅折叠 ASCII 大写字母 A-Z (+32), 其余字符原样。与 Go 端 asciiFold 逐字对齐。
 * 刻意不用 StrLower: 它会对非 ASCII (CJK/全角) 做区域相关变换, 造成双端分歧。
 */
AsciiLower(s) {
  out := ""
  Loop Parse, s {
    o := Ord(A_LoopField)
    if (o >= 65 && o <= 90) {
      out .= Chr(o + 32)
    } else {
      out .= A_LoopField
    }
  }
  return out
}

/**
 * 自定义文本类型匹配: rules 之间 OR, 任一命中即命中 (与 Go 端 matchCustomRules 同语义)。
 * 作用域约定 (方案 C7 核心风控, 两端逐字一致):
 *   - equals / prefix: 作用于 Trim(content) 的首个非空行;
 *   - suffix / contains: 作用于整个 Trim(content);
 * 空 value 的 prefix/suffix/contains 恒真 (镜像 Go 的 HasPrefix/HasSuffix/Contains 对空串返回 true),
 * 该形态由保存校验拒绝, 此处仅为手改配置的一致性兜底。
 * @param rules [{op, value}, ...]
 * @param content 选中文本
 * @returns {boolean}
 */
MatchCustomRules(rules, content) {
  if not (IsObject(rules)) {
    return false
  }
  m := Trim(content, " `t`r`n`v`f")
  first := ""
  for line in StrSplit(m, "`n") {
    line := Trim(line, " `t`r`v`f")
    if (line != "") {
      first := line
      break
    }
  }
  for r in rules {
    op := r.op
    v := AsciiLower(r.value)
    hay := (op == "equals" || op == "prefix") ? AsciiLower(first) : AsciiLower(m)
    switch op {
      case "equals":
        if (hay == v) {
          return true
        }
      case "prefix":
        if (SubStr(hay, 1, StrLen(v)) == v) {
          return true
        }
      case "suffix":
        if (v == "" || (StrLen(hay) >= StrLen(v) && SubStr(hay, -StrLen(v)) == v)) {
          return true
        }
      case "contains":
        if (v == "" || InStr(hay, v)) {
          return true
        }
    }
  }
  return false
}

/**
 * 数组形文件后缀匹配: 语义逐字对齐 MatchFileExt (SplitPath 取末段扩展名 / 去点 /
 * 忽略大小写 / "*" 匹配任意文件 / 无扩展名跳过), 供 type: 文件引用复用。
 * MatchFileExt 本体冻结不动 (注释互指, 语义唯一真源见其文档注释)。
 * @param exts 后缀数组 (不含点)
 * @param content 文件路径列表 (换行分隔)
 * @returns {boolean}
 */
MatchFileExtList(exts, content) {
  if not (IsObject(exts)) {
    return false
  }
  for line in StrSplit(content, "`n") {
    SplitPath(line, , , &ext)
    if not (ext) {
      continue
    }
    for v in exts {
      v := LTrim(Trim(v), ".")
      if v == "*" || AsciiLower(v) == AsciiLower(ext) {
        return true
      }
    }
  }
  return false
}

/**
 * 文件后缀匹配, 条件值支持逗号分隔多个后缀, "*" 匹配任意文件
 * @param matchValue 如 ".txt" / "txt,md" / "*"
 * @param content 文件路径列表 (换行分隔)
 * @returns {boolean}
 */
MatchFileExt(matchValue, content) {
  if matchValue == "*" {
    return true
  }
  exts := StrSplit(matchValue, ",")
  for line in StrSplit(content, "`n") {
    SplitPath(line, , , &ext)
    if not (ext) {
      continue
    }
    for v in exts {
      v := LTrim(Trim(v), ".")
      ; 扩展名比较忽略大小写 (与 Go 端 EqualFold 一致): Windows 上 .JPG/.PNG 等大写扩展名也必须能匹配
      if v == "*" || StrLower(v) == StrLower(ext) {
        return true
      }
    }
  }
  return false
}

/**
 * 内置文本特征注册表 —— AHK 侧唯一真源。
 *
 * 组织方式 (与面板镜像 config-ui-reactor/src/services/selected_action.rs :: TEXT_TYPES
 * **同构** 且同序, 由 devtools texttype-conformance 对账):
 *   - 表的顺序 = 界面顺序 (「添加映射」类型下拉 / 映射行特征 Toggle), **兜底特征恒居末位**;
 *   - named=true  具名特征: 各持一条**锚定**正则, 命中即"属于该特征";
 *   - named=false 兜底特征 (目前仅 plain): **不持正则**, 命中条件由具名集**派生** ——
 *     "其余全部具名特征都不命中"。故新增具名特征时 plain 的排除集自动扩大, 无需手工同步
 *     (2026-09-17 之前这里是硬编码的 `not (isURL or isPath or isMagnet or isBilibili)`,
 *      加第 5 个特征时靠人肉改 —— 正是本次重构要消灭的失败模式);
 *   - ignoreCase 与 pattern 分离: 正则源串与 Go 端**逐字相同**, 大小写开关运行时施加
 *     (AHK 加 "i)" 前缀 / Go 编译期加 (?i)) ⇒ 可工具化比对 (devtools texttype-conformance)。
 *
 * 为什么 plain 必须排除全部具名特征: 映射按数组行序取**首个**命中, 而「添加映射」恒追加到末尾
 * ⇒ 若 plain 也命中某具名特征的样例, 先建的「纯文本」映射会恒遮蔽后建的具名映射 (配了却不生效)。
 *
 * 不要用 $ 收尾: PCRE2 的 $ 还会认末尾换行前的位置, 而 Go 的 $ 只认文本末尾 ⇒ 多行选中时两端分歧;
 * \z 在两侧都表示"文本绝对末尾", 是方言交集。
 *
 * @returns {array} 特征表 (static, 只求值一次)
 */
TextFeatureSpecs() {
  static specs := [
    {value: "url", named: true, ignoreCase: true, pattern: "^(https?|ftp)://"},
    {value: "path", named: true, ignoreCase: false, pattern: "^(\\\\[^\\]+\\[^\\]+|[a-zA-Z]:\\)"},
    {value: "magnet", named: true, ignoreCase: true, pattern: "^magnet:"},
    {value: "bilibili", named: true, ignoreCase: true, pattern: "^(av[0-9]+|bv[0-9a-z]{10})\z"},
    {value: "plain", named: false, ignoreCase: false, pattern: ""},
  ]
  return specs
}

/**
 * 单个特征的命中判定 (fallback 特征的排除集从注册表**派生**, 非硬编码)。
 * @param spec TextFeatureSpecs() 的表项
 * @param content 选中文本 (不 Trim —— 具名特征都是 ^ 锚定, 且该值可能被原样拼进 URL)
 * @returns {boolean}
 */
TextFeatureHit(spec, content) {
  if (spec.named) {
    return RegExMatch(content, (spec.ignoreCase ? "i)" : "") . spec.pattern) > 0
  }
  for other in TextFeatureSpecs() {
    if (other.named and RegExMatch(content, (other.ignoreCase ? "i)" : "") . other.pattern) > 0) {
      return false
    }
  }
  return true
}

/**
 * 文本特征匹配入口 —— 与 Go 端 behaviors.MatchTextFeature 逐字对齐。
 * 特征名归一化: 去首尾空白 (大小写敏感比较, 与旧 switch 同口径 —— AHK 的 switch/`=` 对字符串
 * 是大小写不敏感而 `==` 敏感, 这里刻意用 `==` 保持"配置值必须小写"的既有严格性);
 * content 一律不 Trim (锚定口径)。未知特征名返回 false (与 Go 端同口径)。
 * @param t 特征类型
 * @param content 选中文本
 * @returns {boolean}
 */
MatchTextType(t, content) {
  tv := Trim(t)
  for spec in TextFeatureSpecs() {
    if (spec.value == tv) {
      return TextFeatureHit(spec, content)
    }
  }
  return false
}
