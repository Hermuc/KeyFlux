package script

import (
	"strings"
	"testing"
)

// ============ asciiFold (自写 ASCII 折叠, 禁用 EqualFold/ToLower) ============

func TestAsciiFold(t *testing.T) {
	cases := []struct{ in, want string }{
		{"ABC", "abc"},
		{"Hello, World!", "hello, world!"},
		{"a-b.c", "a-b.c"},   // 非字母不变
		{"ＡＢＣ", "ＡＢＣ"},       // 全角非 ASCII, 不折叠
		{"网盘LINK", "网盘link"}, // 仅 ASCII 大写被折, CJK 原样
		{"🔥FOO", "🔥foo"},     // emoji 原样, 仅 FOO 被折
	}
	for _, c := range cases {
		if got := asciiFold(c.in); got != c.want {
			t.Errorf("asciiFold(%q) = %q, want %q", c.in, got, c.want)
		}
	}
}

// ============ ValidateMatchTypes 表驱动 ============

func TestValidateMatchTypes(t *testing.T) {
	groups := []FileGroup{{Name: "design", Label: "设计", Exts: []string{"psd"}}}
	longVal := strings.Repeat("x", 257) // >256
	t.Run("合法文本类型", func(t *testing.T) {
		ts := []MatchType{{ID: "netdisk", Label: "网盘", Kind: "text",
			Rules: []MatchRule{{Op: "contains", Value: "pan.baidu.com"}}}}
		if err := ValidateMatchTypes(ts, groups); err != nil {
			t.Fatalf("合法文本类型应放行: %v", err)
		}
	})
	t.Run("合法文件类型(kind=fileExt)", func(t *testing.T) {
		ts := []MatchType{{ID: "imgs", Label: "图片", Kind: "fileExt", Exts: []string{"jpg", "png"}}}
		if err := ValidateMatchTypes(ts, groups); err != nil {
			t.Fatalf("合法 fileExt 类型应放行: %v", err)
		}
	})
	t.Run("id 非法(大写)", func(t *testing.T) {
		ts := []MatchType{{ID: "NetDisk", Label: "x", Kind: "text", Rules: []MatchRule{{Op: "contains", Value: "a"}}}}
		if err := ValidateMatchTypes(ts, groups); err == nil || !strings.Contains(err.Error(), "内部标识") {
			t.Fatalf("大写 id 应拒绝: %v", err)
		}
	})
	t.Run("id 非法(数字开头且超长)", func(t *testing.T) {
		ts := []MatchType{{ID: "1" + strings.Repeat("a", 30), Label: "x", Kind: "text", Rules: []MatchRule{{Op: "contains", Value: "a"}}}}
		if err := ValidateMatchTypes(ts, groups); err == nil {
			t.Fatal("超长/数字开头 id 应拒绝")
		}
	})
	t.Run("id 与内置名冲突", func(t *testing.T) {
		ts := []MatchType{{ID: "url", Label: "x", Kind: "text", Rules: []MatchRule{{Op: "contains", Value: "a"}}}}
		if err := ValidateMatchTypes(ts, groups); err == nil || !strings.Contains(err.Error(), "冲突") {
			t.Fatalf("与内置名 url 冲突应拒绝: %v", err)
		}
	})
	t.Run("id 重复", func(t *testing.T) {
		ts := []MatchType{
			{ID: "a", Label: "x", Kind: "text", Rules: []MatchRule{{Op: "contains", Value: "a"}}},
			{ID: "a", Label: "y", Kind: "text", Rules: []MatchRule{{Op: "contains", Value: "b"}}},
		}
		if err := ValidateMatchTypes(ts, groups); err == nil || !strings.Contains(err.Error(), "重复") {
			t.Fatalf("重复 id 应拒绝: %v", err)
		}
	})
	t.Run("id 与文件分组名冲突", func(t *testing.T) {
		ts := []MatchType{{ID: "design", Label: "x", Kind: "text", Rules: []MatchRule{{Op: "contains", Value: "a"}}}}
		if err := ValidateMatchTypes(ts, groups); err == nil || !strings.Contains(err.Error(), "文件分组") {
			t.Fatalf("与文件分组名 design 冲突应拒绝: %v", err)
		}
	})
	t.Run("label 空", func(t *testing.T) {
		ts := []MatchType{{ID: "a", Label: "   ", Kind: "text", Rules: []MatchRule{{Op: "contains", Value: "a"}}}}
		if err := ValidateMatchTypes(ts, groups); err == nil || !strings.Contains(err.Error(), "缺少名称") {
			t.Fatalf("空 label 应拒绝: %v", err)
		}
	})
	t.Run("kind 非法", func(t *testing.T) {
		ts := []MatchType{{ID: "a", Label: "x", Kind: "weird", Rules: []MatchRule{{Op: "contains", Value: "a"}}}}
		if err := ValidateMatchTypes(ts, groups); err == nil || !strings.Contains(err.Error(), "分类") {
			t.Fatalf("非法 kind 应拒绝: %v", err)
		}
	})
	t.Run("text 缺 rules", func(t *testing.T) {
		ts := []MatchType{{ID: "a", Label: "x", Kind: "text"}}
		if err := ValidateMatchTypes(ts, groups); err == nil || !strings.Contains(err.Error(), "匹配条件") {
			t.Fatalf("缺 rules 应拒绝: %v", err)
		}
	})
	t.Run("text 匹配方式非法", func(t *testing.T) {
		ts := []MatchType{{ID: "a", Label: "x", Kind: "text", Rules: []MatchRule{{Op: "regex", Value: "a"}}}}
		if err := ValidateMatchTypes(ts, groups); err == nil || !strings.Contains(err.Error(), "匹配方式") {
			t.Fatalf("非法匹配方式应拒绝: %v", err)
		}
	})
	t.Run("text 值空", func(t *testing.T) {
		ts := []MatchType{{ID: "a", Label: "x", Kind: "text", Rules: []MatchRule{{Op: "contains", Value: "   "}}}}
		if err := ValidateMatchTypes(ts, groups); err == nil || !strings.Contains(err.Error(), "匹配内容为空") {
			t.Fatalf("空值应拒绝: %v", err)
		}
	})
	t.Run("text 值超长", func(t *testing.T) {
		ts := []MatchType{{ID: "a", Label: "x", Kind: "text", Rules: []MatchRule{{Op: "contains", Value: longVal}}}}
		if err := ValidateMatchTypes(ts, groups); err == nil || !strings.Contains(err.Error(), "过长") {
			t.Fatalf("超长值应拒绝: %v", err)
		}
	})
	t.Run("fileExt 缺 exts", func(t *testing.T) {
		ts := []MatchType{{ID: "a", Label: "x", Kind: "fileExt", Exts: []string{".", " "}}}
		if err := ValidateMatchTypes(ts, groups); err == nil || !strings.Contains(err.Error(), "文件扩展名") {
			t.Fatalf("空 exts (归一化后) 应拒绝: %v", err)
		}
	})
}

// ============ CustomMatchTypes 渲染 (空串友好 + 转义) ============

func TestCustomMatchTypesEmpty(t *testing.T) {
	if got := (&Config{}).CustomMatchTypes(); got != "" {
		t.Fatalf("无自定义类型应返回空串 (零 golden 影响落点), 实际 %q", got)
	}
	if got := (&Config{MatchTypes: nil}).CustomMatchTypes(); got != "" {
		t.Fatalf("nil MatchTypes 应返回空串, 实际 %q", got)
	}
}

func TestCustomMatchTypesRendersEscaped(t *testing.T) {
	cfg := &Config{MatchTypes: []MatchType{
		{ID: "netdisk", Label: "网盘", Kind: "text", Rules: []MatchRule{
			{Op: "contains", Value: `he"llo`}, // 含双引号
			{Op: "contains", Value: `x ;y`},   // 含 "空格;"(须转义为 `;)
		}},
		{ID: "design", Label: "设计", Kind: "fileExt", Exts: []string{"psd", "ai"}},
	}}
	got := cfg.CustomMatchTypes()
	if !strings.HasPrefix(got, "global CustomMatchTypes := Map(") {
		t.Fatalf("渲染开头不符: %q", got)
	}
	if !strings.HasSuffix(got, ")") {
		t.Fatalf("渲染结尾不符: %q", got)
	}
	// AhkString 转义: " -> `", " ;" -> " `;
	if !strings.Contains(got, `value: "he`+"`"+`"llo"`) {
		t.Fatalf("双引号转义不正确: %q", got)
	}
	if !strings.Contains(got, `value: "x `+"`"+`;y"`) {
		t.Fatalf("空格后分号转义不正确: %q", got)
	}
	// fileExt 分支渲染 exts 数组
	if !strings.Contains(got, `exts: ["psd", "ai"]`) {
		t.Fatalf("fileExt exts 渲染不符: %q", got)
	}
	// 引号成对: 只计**未转义**的双引号 —— AhkString 会把内容里的 " 转义为 `",
	// 该引号属字符串内容而非定界符, 朴素计数会误报 (本断言初版即如此失败)。
	if unescapedQuoteCount(got)%2 != 0 {
		t.Fatalf("渲染引号未成对: %q", got)
	}
}

// unescapedQuoteCount 统计未被反引号转义的双引号个数 (AHK v2 转义符为 `)。
// 反引号自身可被反引号转义 (连续两个反引号表示一个反引号), 故按连续反引号数的奇偶判定。
func unescapedQuoteCount(s string) int {
	n, backticks := 0, 0
	for _, r := range s {
		switch r {
		case '`':
			backticks++
		case '"':
			if backticks%2 == 0 {
				n++
			}
			backticks = 0
		default:
			backticks = 0
		}
	}
	return n
}

// ============ matchRuleOp / matchCustomRules / matchFileExtList ============

func TestMatchRuleOpBasics(t *testing.T) {
	// equals 取首个非空行
	if !matchRuleOp("equals", "line1", "line1\nline2") {
		t.Error("equals 应命中首行")
	}
	if matchRuleOp("equals", "line1", "line1 extra") {
		t.Error("equals 是整行相等, 不应把前缀当相等")
	}
	// prefix 首行
	if !matchRuleOp("prefix", "pic", "picture.jpg") {
		t.Error("prefix 应命中")
	}
	if matchRuleOp("prefix", "pic", "x\npicture") {
		t.Error("prefix 只看首行, 第二行不计")
	}
	// suffix 整串
	if !matchRuleOp("suffix", ".psd", "a/b/file.PSD") {
		t.Error("suffix ASCII 不敏感应命中")
	}
	// contains 整串
	if !matchRuleOp("contains", "baidu", "https://pan.baidu.com") {
		t.Error("contains 应命中")
	}
	// 未知算子返回 false
	if matchRuleOp("regex", "a", "a") {
		t.Error("未知算子应返回 false")
	}
}

func TestMatchCustomRulesOr(t *testing.T) {
	rules := []MatchRule{
		{Op: "contains", Value: "pan.baidu.com"},
		{Op: "contains", Value: "aliyundrive.com"},
	}
	if !matchCustomRules(rules, "https://pan.baidu.com/x") {
		t.Error("OR: 第一条命中")
	}
	if !matchCustomRules(rules, "https://aliyundrive.com/x") {
		t.Error("OR: 第二条命中")
	}
	if matchCustomRules(rules, "https://example.com") {
		t.Error("OR: 都不命中应返回 false")
	}
	if matchCustomRules(nil, "anything") {
		t.Error("空规则应返回 false")
	}
}

func TestMatchFileExtListAlignsMatchFileExt(t *testing.T) {
	// 数组形与逗号串形逐字对齐 (含通配/去点/忽略大小写/空扩展名跳过)
	pairs := []struct {
		exts    []string
		content string
	}{
		{[]string{"jpg", "png"}, "C:\\a\\b\\c.JPG"},
		{[]string{"*"}, "C:\\any\\file.xyz"},
		{[]string{".psd", ".ai"}, "x.psd"},
		{[]string{"psd", ""}, "x.psd"}, // 空扩展名跳过
		{[]string{"png"}, "just text no ext"},
	}
	for _, p := range pairs {
		listGot := matchFileExtList(p.exts, p.content)
		csvGot := matchFileExt(strings.Join(p.exts, ","), p.content)
		if listGot != csvGot {
			t.Errorf("matchFileExtList %v vs matchFileExt 不一致: content=%q list=%v csv=%v",
				p.exts, p.content, listGot, csvGot)
		}
	}
}

// ============ ResolveMappingValues (type: 引用解析唯一入口) ============

func refConfig() *Config {
	return &Config{
		MatchTypes: []MatchType{
			{ID: "netdisk", Label: "网盘", Kind: "text", Rules: []MatchRule{{Op: "contains", Value: "pan.baidu.com"}}},
		},
		FileGroups: []FileGroup{{Name: "design", Label: "设计", Exts: []string{"psd", "ai", "sketch"}}},
	}
}

func TestResolveMappingValues(t *testing.T) {
	cfg := refConfig()

	// 文本 type: 引用 -> values=[type:netdisk], ok=true
	mt, vals, ok := ResolveMappingValues(cfg, &SelectedMapping{MatchType: "textType", MatchValue: "type:netdisk"})
	if !ok || mt != "textType" || len(vals) != 1 || vals[0] != "type:netdisk" {
		t.Fatalf("文本引用解析错误: %q %v %v", mt, vals, ok)
	}
	// 文本引用悬空 -> ok=false
	if _, _, ok := ResolveMappingValues(cfg, &SelectedMapping{MatchType: "textType", MatchValue: "type:ghost"}); ok {
		t.Fatal("悬空文本引用应 ok=false")
	}
	// 文件 type: 引用 -> 解析为分组后缀集
	mt, vals, ok = ResolveMappingValues(cfg, &SelectedMapping{MatchType: "fileExt", MatchValue: "type:design"})
	if !ok || mt != "fileExt" || len(vals) != 3 {
		t.Fatalf("文件引用解析错误: %q %v %v", mt, vals, ok)
	}
	// 文件引用悬空 -> ok=false
	if _, _, ok := ResolveMappingValues(cfg, &SelectedMapping{MatchType: "fileExt", MatchValue: "type:missing"}); ok {
		t.Fatal("悬空文件引用应 ok=false")
	}
	// 内置文本特征 (非引用) -> 透传单值
	mt, vals, ok = ResolveMappingValues(cfg, &SelectedMapping{MatchType: "textType", MatchValue: "url"})
	if !ok || mt != "textType" || vals[0] != "url" {
		t.Fatalf("内置文本特征透传错误: %q %v %v", mt, vals, ok)
	}
	// 未知内置文本特征 -> ok=false
	if _, _, ok := ResolveMappingValues(cfg, &SelectedMapping{MatchType: "textType", MatchValue: "hash"}); ok {
		t.Fatal("未知文本特征应 ok=false")
	}
	// 文件后缀字面串 -> 展开后缀集
	mt, vals, ok = ResolveMappingValues(cfg, &SelectedMapping{MatchType: "fileExt", MatchValue: "jpg,png"})
	if !ok || mt != "fileExt" || len(vals) != 2 {
		t.Fatalf("文件字面串解析错误: %q %v %v", mt, vals, ok)
	}
	// 空文件后缀 -> ok=false
	if _, _, ok := ResolveMappingValues(cfg, &SelectedMapping{MatchType: "fileExt", MatchValue: ""}); ok {
		t.Fatal("空文件后缀应 ok=false")
	}
	// cfg=nil + 引用 -> ok=false (容忍)
	if _, _, ok := ResolveMappingValues(nil, &SelectedMapping{MatchType: "textType", MatchValue: "type:netdisk"}); ok {
		t.Fatal("nil 注册表下引用应 ok=false")
	}
}
