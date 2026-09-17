package behaviors

import (
	"fmt"
	"regexp"
	"strings"
)

// ============================ 内置文本特征注册表 (唯一真源) ============================
//
// 组织方式
//   1. 顺序即语义顺序 —— 「添加映射」的类型下拉、映射行的特征 Toggle 都按本表顺序渲染;
//      兜底特征 (plain) 恒居末位。共享向量 config-server/internal/script/testdata/text_types.json
//      的 `types` 与本表同序, 由 Go 侧 TestTextTypeVector_* 与 tools/texttype_conformance.py 双向钉死。
//   2. 具名特征 (Named=true) 各持一条**锚定**正则, 命中即"属于该特征"。
//   3. 兜底特征 (Fallback=true, 目前仅 plain) **不持正则**: 它的命中条件由具名集**派生** ——
//      "其余全部具名特征都不命中"。故新增具名特征时 plain 的排除集自动扩大, 无需手工同步
//      (2026-09-17 之前该排除集是手写的 `!reURL && !rePath && !reMagnet`, 加第 5 个特征时靠人肉改)。
//   4. IgnoreCase 与 Pattern 分离: 正则源串在两端**逐字相同**, 大小写开关由各端自行施加
//      (Go 编译期加 `(?i)`, AHK 运行期加 `"i)"` 前缀), 使 tools 能对 Pattern 做字面比对。
//      新增特征时**不要**把 `(?i)` 写进 Pattern —— 写进 IgnoreCase。
//
// 为什么 plain 必须排除全部具名特征
//   映射按数组行序取**首个**命中, 而「添加映射」恒追加到末尾 ⇒ 若 plain 也命中某具名特征的样例,
//   先建的「纯文本」映射会恒遮蔽后建的具名映射, 表现为"配了却不生效"。故排除集必须恒等于具名集。
//
// 为什么用 \z 而非 $ 收尾
//   Go 的 `$` 只认文本末尾; PCRE2 的 `$` 还认末尾换行前的位置 ⇒ 多行选中时两端会分歧。
//   `\z` 在两侧都表示"文本绝对末尾", 是两端方言的交集 (详见 tools/texttype_conformance.py)。
//
// 新增一个内置文本特征的接入点 (共 4 处, 详见 docs/design-text-feature-registry.md)
//   ① 本表加一行 (值/名/正则/大小写)            —— 本文件
//   ② AHK 侧 TextFeatureSpecs() 加同一行        —— bin/lib/rules/SelectedAction.ahk
//   ③ 该特征下的可用行为包                      —— bin/behaviors/<id>/behavior.json
//   ④ 界面文案 (i18n 新键 + Toggle) 与向量用例   —— i18n.json / SelectedActionPageView.axaml / testdata/text_types.json
//   Go / AHK / 向量 三者的一致性由 `make check-texttypes` 强制, 漏改必红。

// TextFeature 单个内置文本特征的声明。Named 与 Fallback 互斥且必居其一 (init 自检)。
type TextFeature struct {
	Value      string // 配置值 / 稳定 id (小写; 即 config.json 里的 matchValue)
	Label      string // 中文名 (保存期错误文案拼接用; 与 i18n 1059-1062 / 2580 同义)
	Named      bool   // true=具名特征, 参与 plain 的排除集
	Fallback   bool   // true=兜底特征 (命中条件 = 其余具名特征全不命中)
	IgnoreCase bool   // 正则大小写不敏感 (Go 编译期加 (?i) / AHK 运行期加 "i)" 前缀)
	Pattern    string // 正则源串 (RE2 与 PCRE2 的方言交集; 兜底项为空)
	re         *regexp.Regexp
}

var textFeatures = []TextFeature{
	{Value: "url", Label: "链接", Named: true, IgnoreCase: true, Pattern: `^(https?|ftp)://`},
	{Value: "path", Label: "路径", Named: true, Pattern: `^(\\\\[^\\]+\\[^\\]+|[a-zA-Z]:\\)`},
	{Value: "magnet", Label: "磁力链接", Named: true, IgnoreCase: true, Pattern: `^magnet:`},
	{Value: "bilibili", Label: "B 站", Named: true, IgnoreCase: true, Pattern: `^(av[0-9]+|bv[0-9a-z]{10})\z`},
	{Value: "plain", Label: "纯文本", Fallback: true},
}

// init 编译正则并自检注册表结构 —— 配置错误必须在进程启动时炸掉, 而不是运行时静默不命中。
func init() {
	last := len(textFeatures) - 1
	for i := range textFeatures {
		f := &textFeatures[i]
		if f.Named == f.Fallback {
			panic(fmt.Sprintf("文本特征注册表 %q: Named 与 Fallback 必须恰有一个为 true", f.Value))
		}
		if f.Named {
			if f.Pattern == "" {
				panic(fmt.Sprintf("文本特征注册表 %q: 具名特征必须给出 Pattern", f.Value))
			}
			p := f.Pattern
			if f.IgnoreCase {
				p = "(?i)" + p
			}
			f.re = regexp.MustCompile(p)
		} else if i != last {
			// plain 的"其余都不命中"语义要求它在**行序判定**中不抢先短路任何具名特征。
			// 它本身不是正则, 放中间也不会误命中, 但界面顺序会跟着乱 ⇒ 强制居末。
			panic(fmt.Sprintf("文本特征注册表 %q: 兜底特征必须居末位", f.Value))
		}
	}
}

// TextFeatures 返回注册表只读视图 (顺序即界面顺序, 兜底项居末)。
func TextFeatures() []TextFeature { return textFeatures }

// TextFeatureValues 注册表值表 (顺序同上)。供 reservedTextTypeNames / 一致性测试消费。
func TextFeatureValues() []string {
	out := make([]string, 0, len(textFeatures))
	for i := range textFeatures {
		out = append(out, textFeatures[i].Value)
	}
	return out
}

// TextFeatureLabels 注册表中文名表 (顺序同上)。
func TextFeatureLabels() []string {
	out := make([]string, 0, len(textFeatures))
	for i := range textFeatures {
		out = append(out, textFeatures[i].Label)
	}
	return out
}

// TextFeatureHint 把中文名拼成错误文案里的可选值提示: "链接 / 路径 / 磁力链接 / B 站 / 纯文本"。
// 由注册表派生 ⇒ 新增特征时错误文案自动跟上, 不会再出现"词表加了但提示还是老的"。
func TextFeatureHint() string { return strings.Join(TextFeatureLabels(), " / ") }

// FindTextFeature 按配置值查表 (归一化后精确匹配; 大小写不敏感、去首尾空白)。
// 返回 nil 表示不是内置特征 (可能是 type:<id> 自定义引用, 由调用方另判)。
func FindTextFeature(value string) *TextFeature {
	v := strings.ToLower(strings.TrimSpace(value))
	for i := range textFeatures {
		if textFeatures[i].Value == v {
			return &textFeatures[i]
		}
	}
	return nil
}

// IsKnownTextType 是否为内置文本特征值 (含 plain)。
func IsKnownTextType(value string) bool { return FindTextFeature(value) != nil }

// TextFeatureHit 单个特征的命中判定。fallback 特征排除**全部**具名特征 (派生, 非硬编码)。
func TextFeatureHit(f *TextFeature, content string) bool {
	if f == nil {
		return false
	}
	if !f.Fallback {
		return f.re.MatchString(content)
	}
	for i := range textFeatures {
		g := &textFeatures[i]
		if g.Named && g.re.MatchString(content) {
			return false
		}
	}
	return true
}

// MatchTextFeature 文本特征匹配入口 (与 AHK 端 MatchTextType 逐字对齐)。
// 特征名归一化: 去首尾空白 + 转小写; content 一律**不做** Trim —— 具名特征都是 ^ 锚定,
// 该值还可能被原样拼进 URL (见 bin/behaviors/open_bilibili), 容忍空白会生成非法链接。
// 未知特征名返回 false (与旧 switch 的 default 分支同口径)。
func MatchTextFeature(value, content string) bool {
	return TextFeatureHit(FindTextFeature(value), content)
}
