package script

import (
	"path/filepath"
	"strings"
	"testing"

	"settings/internal/behaviors"
)

// ============ matchTextType: 5 个内置文本特征的单一真源语义 ============
//
// 与 AHK 端 bin/lib/rules/SelectedAction.ahk 的 MatchTextType **逐条对齐** —— 任一侧改动必须双端同批,
// 否则生成端 (Go 匹配/校验/模拟测试) 与运行时 (AHK) 会给出不同答案。
// bilibili 为 2026-09-17 新增特征 (AV 号 / BV 号)。

func TestMatchTextType_BuiltinValues(t *testing.T) {
	cases := []struct {
		content string
		want    string
	}{
		{"https://github.com/Hermuc/KeyFlux", "url"},
		{"ftp://example.com/x", "url"},
		{`C:\Windows\explorer.exe`, "path"},
		{`\\server\share\a.txt`, "path"},
		{"magnet:?xt=urn:btih:abc", "magnet"},
		{"av170001", "bilibili"},
		{"BV1xx411c7mD", "bilibili"},
		{"hello world", "plain"},
		{"这是一段纯文本", "plain"},
	}
	for _, c := range cases {
		if !matchTextType(c.want, c.content) {
			t.Errorf("matchTextType(%q, %q) = false, want true", c.want, c.content)
		}
	}
	// 大小写与首尾空白: 特征名走 ToLower+Trim; 内容一律不 Trim (锚定口径与既有 4 个特征一致)
	if !matchTextType("BILIBILI", "av170001") {
		t.Error("特征名大小写应被归一 (BILIBILI -> bilibili)")
	}
	if matchTextType("bilibili", " av170001") {
		t.Error("内容含前导空格不应命中 (该值会被原样拼进视频 URL)")
	}
}

// B 站号的命中面: AV 号 (av+数字) 与 BV 号 (bv/BV + 10 位 [0-9A-Za-z]), 必须**整串**命中。
func TestMatchTextType_Bilibili(t *testing.T) {
	hits := []string{
		"av170001",
		"AV170001",
		"av2",
		"BV1xx411c7mD",
		"bv1xx411c7md",
		"BV1xx411c7m0",
	}
	for _, c := range hits {
		if !matchTextType("bilibili", c) {
			t.Errorf("matchTextType(bilibili, %q) = false, want true", c)
		}
		// plain 必须排除 B 站号: 映射按数组行序取首个命中, 而「添加映射」恒追加到末尾 ⇒
		// 若 plain 也命中, 先建的「纯文本」映射会恒遮蔽后建的「B 站」映射 (配了却不生效)。
		if matchTextType("plain", c) {
			t.Errorf("matchTextType(plain, %q) = true, 但 B 站号必须从 plain 排除", c)
		}
	}

	misses := []string{
		"av",                 // 缺数字
		"av12x",              // 混字母
		"av 170001",          // 含空格
		"BV1xx411c7m",        // BV 后仅 9 位
		"BV1xx411c7mDx",      // BV 后 11 位
		"BV1xx411c7m_",       // 下划线不在 [0-9A-Za-z]
		"1xx411c7mD",         // 缺 BV 前缀
		"av170001 BV1xx411c", // 两个号拼接
	}
	for _, c := range misses {
		if matchTextType("bilibili", c) {
			t.Errorf("matchTextType(bilibili, %q) = true, want false", c)
		}
	}

	// 完整 B 站链接不侵占既有特征: 仍归 url (由「默认浏览器打开网址」处理)
	const link = "https://www.bilibili.com/video/BV1xx411c7mD"
	if matchTextType("bilibili", link) || !matchTextType("url", link) {
		t.Error("完整 B 站链接应命中 url 而非 bilibili")
	}
}

// ============ 行为包前置: 新特征必须在「添加映射」弹窗里有可用行为 ============
//
// 方案 C7 的死胡同: 特征值没有行为覆盖时, 弹窗勾选列表为空且确认按钮禁用。
// 本夹具刻意读**真实** bin/behaviors (而非内联夹具): 新增内置特征却漏建行为包时,
// 其余 Go 单测会全绿, 缺陷只在用户界面里暴露 —— 故必须在此处兜住。
func TestBuiltinCatalog_CoversBilibili(t *testing.T) {
	cat := behaviors.LoadCatalog(filepath.Join("..", "..", "..", "bin", "behaviors"), filepath.Join(t.TempDir(), "user"))
	if len(cat.Errors) > 0 {
		t.Fatalf("内置行为包目录加载有错: %v", cat.Errors)
	}
	if !cat.Covers("open_bilibili", "textType", []string{"bilibili"}) {
		t.Fatal("内置行为 open_bilibili 未声明 appliesTo textType=bilibili")
	}
	if got := cat.DefaultFor("textType", []string{"bilibili"}); got != "open_bilibili" {
		t.Fatalf("textType=bilibili 的默认行为 = %q, want open_bilibili", got)
	}
	// 模板必须把选中值拼进 B 站视频地址: entry.action 是通用的 open (RunReplaced),
	// 模板缺失时会把视频号当命令直接 Run。
	p := cat.Get("open_bilibili")
	if p == nil || !strings.Contains(p.Entry.Params.ActionValue, "https://www.bilibili.com/video/") ||
		!strings.Contains(p.Entry.Params.ActionValue, "%selected%") {
		t.Fatalf("open_bilibili 的 entry.params.actionValue 未把 %%selected%% 拼进 B 站视频地址: %+v", p)
	}
}
