package script

import (
	"fmt"
	"regexp"
	"strings"
	"unicode/utf8"

	"settings/internal/behaviors"
)

// matchTypeName 匹配类型中文显示名 (错误提示用)
func matchTypeName(matchType string) string {
	if matchType == "textType" {
		return "文本特征"
	}
	return "文件后缀"
}

// behaviorDisplayName 行为显示名: 优先包名, 缺失回退原始 ID
func behaviorDisplayName(cat *behaviors.Catalog, id string) string {
	if cat != nil {
		if p := cat.Get(id); p != nil {
			return p.Name
		}
	}
	return id
}

// ValidateFileGroups 校验文件分组表结构: 名称/显示名非空, 后缀列表非空 (分组为快捷填充数据, 结构非法时拒绝保存)
func ValidateFileGroups(groups []FileGroup) error {
	// Name 升格为"文件后缀匹配类型"的稳定标识 (方案 C7): 须匹配命名空间约定,
	// 才能安全嵌入 type:<Name> 引用 (Windows 文件名非法字符天然保证无碰撞)。
	nameRe := regexp.MustCompile(`^[a-z][a-z0-9_]*$`)
	for i := range groups {
		g := &groups[i]
		if strings.TrimSpace(g.Name) == "" {
			return fmt.Errorf("文件分组第 %d 项缺少名称 (name)", i+1)
		}
		if !nameRe.MatchString(g.Name) {
			return fmt.Errorf("文件分组「%s」名称不合法 (须以小写字母开头, 仅含小写字母/数字/下划线)", g.Name)
		}
		if strings.TrimSpace(g.Label) == "" {
			return fmt.Errorf("文件分组「%s」缺少显示名 (label)", g.Name)
		}
		if len(g.Exts) == 0 {
			return fmt.Errorf("文件分组「%s」的后缀列表 (exts) 为空", g.Name)
		}
		for j, ext := range g.Exts {
			// 分组会随 CustomMatchTypes() 渲染进 AHK 字面量: 控制字符会让生成产物语法破损,
			// 故保存期直接拒绝 (与 AhkString 只转义 ` " ; 的能力边界对应)。
			if hasControlChar(ext) {
				return fmt.Errorf("文件分组「%s」第 %d 个后缀含换行等控制字符", g.Name, j+1)
			}
		}
	}
	return nil
}

// hasControlChar 判断字符串是否含换行类控制字符 —— 这类字符无法由 AhkString 转义
// (它只处理 ` " ;), 一旦进入生成产物会破坏 AHK 语法, 必须在保存期拒绝。
func hasControlChar(s string) bool {
	return strings.ContainsAny(s, "\r\n")
}

// MatchSelectedAction 及其匹配语义已迁至 selectedaction.go (单键分发); 本文件保留
// matchActionRule/matchFileExt/matchTextType 供其复用。
// cfg 为可选注册表 (自定义类型表): 内置值走原分支一字不改, 仅 `type:` 引用走解析路径。
func matchActionRule(c *Config, rule *ActionRule, isFile bool, content string) bool {
	switch rule.MatchType {
	case "fileExt":
		if !isFile {
			return false
		}
		if isCustomRef(rule.MatchValue) {
			if c == nil {
				return false
			}
			id := strings.TrimPrefix(rule.MatchValue, "type:")
			// 解析顺序必须与 AHK 端 (CustomMatchTypes 表) 和 behaviors.matchTypeResolver 一致:
			// matchTypes (kind=fileExt) 优先, 再退到 fileGroups —— 两者共用同一 type: 命名空间
			// (ValidateMatchTypes 保证 id 不与分组名冲突, 故顺序无歧义); 未命中即不命中。
			if mt := c.FindMatchType(id); mt != nil && mt.Kind == "fileExt" {
				return matchFileExtList(mt.Exts, content)
			}
			exts, ok := c.FileGroupExts(id)
			if !ok {
				return false
			}
			return matchFileExtList(exts, content)
		}
		return matchFileExt(rule.MatchValue, content)
	case "textType":
		if isFile {
			return false
		}
		if isCustomRef(rule.MatchValue) {
			if c == nil {
				return false
			}
			id := strings.TrimPrefix(rule.MatchValue, "type:")
			mt := c.FindMatchType(id)
			if mt == nil || mt.Kind != "text" {
				return false
			}
			return matchCustomRules(mt.Rules, content)
		}
		return matchTextType(rule.MatchValue, content)
	}
	return false
}

// isCustomRef 判断映射值是否为自定义类型引用 (type: 命名空间前缀)。
// 约定与 behaviors.IsCustomRef 同义 (纯前缀判断, 两处各一份, 无循环依赖);
// `:` 为 Windows 文件名非法字符, 故不可能与任何真实后缀或内置值碰撞。
func isCustomRef(v string) bool {
	return strings.HasPrefix(v, "type:")
}

// matchFileExt 匹配文件后缀, MatchValue 支持逗号分隔多个后缀, "*" 匹配任意文件
func matchFileExt(matchValue, content string) bool {
	if matchValue == "*" {
		return true
	}
	exts := strings.Split(matchValue, ",")
	for _, line := range strings.Split(content, "\n") {
		// fileExt 返回含点后缀, 去掉点后与条件值比较 (与 AHK 端 SplitPath 返回不带点扩展名的语义一致)
		ext := strings.TrimPrefix(fileExt(line), ".")
		if ext == "" {
			continue
		}
		for _, v := range exts {
			v = strings.TrimSpace(v)
			v = strings.TrimPrefix(v, ".")
			if v == "*" || strings.EqualFold(v, ext) {
				return true
			}
		}
	}
	return false
}

// matchTextType 匹配内置文本特征 (url/path/magnet/bilibili/plain)。
//
// 实现已迁至 behaviors/textfeatures.go 的**注册表**: 该表是内置特征的唯一真源
// (值 / 中文名 / 正则 / 大小写开关 / 具名与兜底之分), 本函数只是分派壳。
// plain 的排除集由注册表**派生** ("其余具名特征全不命中"), 新增特征时无需改本文件 ——
// 2026-09-17 之前这里是硬编码的 4 项布尔与, 每次加特征都要人肉同步。
// 与 AHK 端 MatchTextType 的一致性由 testdata/text_types.json + tools/texttype_conformance.py 强制。
func matchTextType(t, content string) bool {
	return behaviors.MatchTextFeature(t, content)
}

// ============================ 自定义匹配类型: 文本算子 ============================
// 算子词表封闭为 4 个 (equals/prefix/suffix/contains), 与 AHK MatchCustomRules 逐字对齐。
// 大小写处理自写 asciiFold (仅折 A-Z), 禁止用 strings.EqualFold/ToLower —— 与既有的
// fileExt 非 ASCII 折叠瑕疵刻意划清, 杜绝历史 RE2/PCRE2 正则分歧重演 (doc/与原版的差异.md)。

// asciiFold 仅把 ASCII 大写字母 A-Z 折为小写 (+32), 其余字节原样; 非 ASCII (CJK/全角/emoji)
// 不折叠、不修改, 两端语义一致。对应 AHK AsciiLower (逐字符 A-Z -> +32)。
func asciiFold(s string) string {
	b := []byte(s)
	for i := range b {
		if b[i] >= 'A' && b[i] <= 'Z' {
			b[i] += 32
		}
	}
	return string(b)
}

// firstNonEmptyLine 返回 Trim(content) 的首个非空行 (equals/prefix 算子的作用域)。
func firstNonEmptyLine(content string) string {
	for _, line := range strings.Split(content, "\n") {
		line = strings.TrimSpace(line)
		if line != "" {
			return line
		}
	}
	return ""
}

// matchRuleOp 单个算子的匹配分派 (供一致性向量测试直调, 也是 matchCustomRules 的原子单元)。
// 作用域约定 (方案 C7 核心风控, 两端必须逐字一致):
//   - equals/prefix: 作用于 Trim(content) 的首个非空行;
//   - suffix/contains: 作用于整个 Trim(content)。
func matchRuleOp(op, value, content string) bool {
	m := strings.TrimSpace(content)
	first := firstNonEmptyLine(m)
	fv := asciiFold(value)
	switch op {
	case "equals":
		return asciiFold(first) == fv
	case "prefix":
		return strings.HasPrefix(asciiFold(first), fv)
	case "suffix":
		return strings.HasSuffix(asciiFold(m), fv)
	case "contains":
		return strings.Contains(asciiFold(m), fv)
	}
	return false
}

// matchCustomRules 自定义文本类型的 OR 求值: 任一规则命中即命中。
func matchCustomRules(rules []MatchRule, content string) bool {
	for _, r := range rules {
		if matchRuleOp(r.Op, r.Value, content) {
			return true
		}
	}
	return false
}

// matchFileExtList 数组形后缀匹配, 语义逐字对齐 matchFileExt (去点/忽略大小写/`*` 任意/空扩展名跳过)。
// 供 type: 文件引用 (解析为文件分组后缀集) 复用, 不改动 matchFileExt 本体。
func matchFileExtList(exts []string, content string) bool {
	for _, line := range strings.Split(content, "\n") {
		ext := strings.TrimPrefix(fileExt(line), ".")
		if ext == "" {
			continue
		}
		for _, v := range exts {
			v = strings.TrimSpace(v)
			v = strings.TrimPrefix(v, ".")
			if v == "*" || strings.EqualFold(v, ext) {
				return true
			}
		}
	}
	return false
}

// normalizeExts 归一化后缀集: 去空白、去两端点、丢弃空串 (镜像 behaviors.normalizeExt 语义)。
func normalizeExts(exts []string) []string {
	var out []string
	for _, e := range exts {
		if e = strings.Trim(strings.TrimSpace(e), "."); e != "" {
			out = append(out, e)
		}
	}
	return out
}

// matchTypeIDRe 自定义类型标识约束: 小写字母开头, 仅含小写字母/数字/下划线, 长度 1-24。
var matchTypeIDRe = regexp.MustCompile(`^[a-z][a-z0-9_]{0,23}$`)

// reservedTextTypeNames 内置文本特征名, 自定义类型不得占用 (避免与内置匹配前提混淆)。
// 由 behaviors 注册表**派生** (唯一真源: behaviors/textfeatures.go), 新增内置特征无需改本文件。
var reservedTextTypeNames = makeReservedTextTypeNames()

func makeReservedTextTypeNames() map[string]bool {
	vs := behaviors.TextFeatureValues()
	out := make(map[string]bool, len(vs))
	for _, v := range vs {
		out[v] = true
	}
	return out
}

// validMatchOps 封闭 4 算子。
var validMatchOps = map[string]bool{"equals": true, "prefix": true, "suffix": true, "contains": true}

// ValidateMatchTypes 校验自定义匹配类型表 (保存期严格; 加载期已容忍, 见方案 D.1.4)。
// 规则: id 合法/唯一/不与内置名及文件分组名冲突; label 非空; kind ∈ {text,fileExt};
// kind=text 时 rules ≥1 且每条 op 合法、value 非空且 ≤256 字符; kind=fileExt 时 exts 归一化后非空。
// 错误信息中文。
func ValidateMatchTypes(ts []MatchType, groups []FileGroup) error {
	seen := make(map[string]bool, len(ts))
	groupNames := make(map[string]bool, len(groups))
	for _, g := range groups {
		groupNames[g.Name] = true
	}
	for i := range ts {
		mt := &ts[i]
		id := strings.TrimSpace(mt.ID)
		// 文案口径 (2026-09-15): 面向普通用户的规范书面语（避免口语, 也避免算子/后缀集等内部术语）——
		// 标识→内部标识, 算子→匹配方式, 后缀列表→文件扩展名; 与设置界面「匹配类型」弹窗逐条对齐 (见实施计划 §8.1)。
		if !matchTypeIDRe.MatchString(id) {
			return fmt.Errorf("内部标识「%s」格式不正确（需以小写字母开头，仅含小写字母、数字与下划线，长度 1-24）", mt.ID)
		}
		if reservedTextTypeNames[id] {
			return fmt.Errorf("内部标识「%s」与内置文本特征（链接／路径／磁力链接／纯文本）冲突，请更换", id)
		}
		if seen[id] {
			return fmt.Errorf("内部标识「%s」重复，同一类型的标识必须唯一", id)
		}
		seen[id] = true
		if groupNames[id] {
			return fmt.Errorf("内部标识「%s」与文件分组同名，请更换", id)
		}
		if strings.TrimSpace(mt.Label) == "" {
			return fmt.Errorf("匹配类型「%s」缺少名称", id)
		}
		if mt.Kind != "text" && mt.Kind != "fileExt" {
			return fmt.Errorf("匹配类型「%s」的分类无效（可选：文本内容 / 文件类型）", id)
		}
		if mt.Kind == "text" {
			if len(mt.Rules) == 0 {
				return fmt.Errorf("文本类型「%s」缺少匹配条件，至少需要 1 条", id)
			}
			for j, r := range mt.Rules {
				if !validMatchOps[r.Op] {
					return fmt.Errorf("匹配类型「%s」第 %d 条匹配条件的匹配方式无效「%s」（可选：包含该文字 / 完全相同 / 以该文字开头 / 以该文字结尾）", id, j+1, r.Op)
				}
				if strings.TrimSpace(r.Value) == "" {
					return fmt.Errorf("匹配类型「%s」第 %d 条匹配内容为空", id, j+1)
				}
				if utf8.RuneCountInString(r.Value) > 256 {
					return fmt.Errorf("匹配类型「%s」第 %d 条匹配内容过长（最多 256 个字符）", id, j+1)
				}
				if hasControlChar(r.Value) {
					return fmt.Errorf("匹配类型「%s」第 %d 条匹配内容不能包含换行符", id, j+1)
				}
			}
		} else { // fileExt
			if len(normalizeExts(mt.Exts)) == 0 {
				return fmt.Errorf("文件类型「%s」缺少文件扩展名（如 psd, ai）", id)
			}
			for j, ext := range mt.Exts {
				// 与 ValidateFileGroups 同口径: exts 会渲染进 AHK 字面量, 控制字符必须拒绝
				if hasControlChar(ext) {
					return fmt.Errorf("匹配类型「%s」第 %d 个文件扩展名不能包含换行符", id, j+1)
				}
			}
		}
	}
	return nil
}

// PreviewAction 生成执行预览: 把 %selected% 替换为选中内容, search 类型返回 URL 编码后的结果
func PreviewAction(rule *ActionRule, content string) string {
	switch rule.ActionType {
	case "search":
		return strings.ReplaceAll(rule.ActionValue, "%selected%", urlEncode(content))
	case "open_url":
		return "用默认浏览器打开: " + content
	case "open_path":
		return "按系统关联程序打开: " + content
	case "open_folder":
		return "打开选中路径所在文件夹"
	case "magnet_download":
		return "用默认 BT 下载工具下载: " + content
	default:
		return strings.ReplaceAll(rule.ActionValue, "%selected%", content)
	}
}

// urlEncode 与 AHK 端 URIEncode (Functions.ahk) 行为一致: 非保留字符原样输出, 其余按字节百分号编码
func urlEncode(s string) string {
	var buf strings.Builder
	for _, b := range []byte(s) {
		if (b >= '0' && b <= '9') || (b >= 'A' && b <= 'Z') || (b >= 'a' && b <= 'z') {
			buf.WriteByte(b)
		} else {
			buf.WriteString(fmt.Sprintf("%%%02X", b))
		}
	}
	return buf.String()
}

// fileExt 取文件后缀 (含点, 如 ".txt"), 无后缀返回空字符串
func fileExt(path string) string {
	path = strings.TrimSpace(path)
	idx := strings.LastIndexByte(path, '.')
	if idx < 0 || idx == len(path)-1 {
		return ""
	}
	return path[idx:]
}
