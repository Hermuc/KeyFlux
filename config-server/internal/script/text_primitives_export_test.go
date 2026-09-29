package script

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"settings/internal/script/generators"
	"settings/internal/script/model"
)

// TestExportTextPrimitives 导出「AHK 文本层」原语的 Go 参考实现输出, 供 Rust 迁移端逐值对账 (P3)。
//
//	cd config-server && UPDATE_TEXT_FIXTURE=1 go test ./internal/script/ -run TestExportTextPrimitives
//
// 为什么需要它: 这些函数是**跨端必须逐字相同**的纯函数 —— AhkString 的转义**顺序**、
// divide 的浮点格式 (Go `%.3f` vs Rust `{:.3}`)、NotBlankLines 的 trim 语义, 任一处的
// 细微差别都会让 KeyFlux.ahk 字节不等。手写期望值等于自证; 这里取 Go 的真实输出。
//
// 口径: 只经**导出**入口取值 (TemplateFuncMap / model 导出函数)。未导出的 substr /
// containsOnlyModifier 无法从这里取, 它们在 Rust 侧按源码移植, 由端到端 parity 兜底。
func TestExportTextPrimitives(t *testing.T) {
	if os.Getenv("UPDATE_TEXT_FIXTURE") != "1" {
		t.Skip("set UPDATE_TEXT_FIXTURE=1 to export config-ui-reactor/tests/fixtures/text_primitives.json")
	}

	type strCase struct {
		In  string `json:"in"`
		Out string `json:"out"`
	}
	type linesCase struct {
		In  string   `json:"in"`
		Out []string `json:"out"`
	}
	type divCase struct {
		A   int    `json:"a"`
		B   int    `json:"b"`
		Out string `json:"out"`
	}
	type joinCase struct {
		Sep   string   `json:"sep"`
		Elems []string `json:"elems"`
		Out   string   `json:"out"`
	}

	type concatCase struct {
		A   string `json:"a"`
		B   string `json:"b"`
		Out string `json:"out"`
	}

	divide := generators.TemplateFuncMap["divide"].(func(int, int) string)
	concat := generators.TemplateFuncMap["concat"].(func(string, string) string)
	escape := generators.TemplateFuncMap["escapeAhkHotkey"].(func(string) string)
	joinFn := generators.TemplateFuncMap["join"].(func(string, []interface{}) string)

	fixture := map[string]any{}

	strIn := []string{"", "abc", "a`b", `a"b`, "a ;b", "a;b", " ;", "a\nb", "全角；", "a `; b"}
	ahk := make([]strCase, 0, len(strIn))
	for _, s := range strIn {
		ahk = append(ahk, strCase{In: s, Out: model.AhkString(s)})
	}
	fixture["ahk_string"] = ahk

	argIn := []string{"plain", "ahk-expression:  WinActive(\"A\")  ", "ahk-expression:foo bar", "ahk-expressions:x", "ahk-expression: ;"}
	args := make([]strCase, 0, len(argIn))
	for _, s := range argIn {
		args = append(args, strCase{In: s, Out: model.ToAHKFuncArg(s)})
	}
	fixture["to_ahk_func_arg"] = args

	lineIn := []string{"a\n\n b \n\t\nc", "", "  \n\n", "x", "  leading\n\ttab  "}
	lines := make([]linesCase, 0, len(lineIn))
	for _, s := range lineIn {
		lines = append(lines, linesCase{In: s, Out: model.NotBlankLines(s)})
	}
	fixture["not_blank_lines"] = lines

	divIn := [][2]int{{1000, 1000}, {1, 3}, {2, 3}, {0, 5}, {-1, 2}, {1500, 1000}, {1, 1000}, {999, 1000}, {1234, 1000}, {1, 0}, {500, 1000}, {2000, 1000}}
	divs := make([]divCase, 0, len(divIn))
	for _, c := range divIn {
		divs = append(divs, divCase{A: c[0], B: c[1], Out: divide(c[0], c[1])})
	}
	fixture["divide"] = divs

	concatIn := [][2]string{{"a", "b"}, {"", "x"}, {"x", ""}, {"全", "角"}}
	cat := make([]concatCase, 0, len(concatIn))
	for _, c := range concatIn {
		cat = append(cat, concatCase{A: c[0], B: c[1], Out: concat(c[0], c[1])})
	}
	fixture["concat"] = cat

	escapeIn := []string{";", "q", "", "#!^", "`;"}
	esc := make([]strCase, 0, len(escapeIn))
	for _, s := range escapeIn {
		esc = append(esc, strCase{In: s, Out: escape(s)})
	}
	fixture["escape_ahk_hotkey"] = esc

	joins := []joinCase{
		{Sep: "-", Elems: []string{"a", "b"}, Out: joinFn("-", []interface{}{"a", "b"})},
		{Sep: "", Elems: []string{"a", "b"}, Out: joinFn("", []interface{}{"a", "b"})},
		{Sep: ", ", Elems: []string{"one"}, Out: joinFn(", ", []interface{}{"one"})},
	}
	fixture["join"] = joins

	raw, err := json.MarshalIndent(fixture, "", "  ")
	if err != nil {
		t.Fatalf("序列化失败: %v", err)
	}
	raw = append(raw, '\n')

	out := filepath.Join("..", "..", "..", "config-ui-reactor", "tests", "fixtures", "text_primitives.json")
	if err := os.MkdirAll(filepath.Dir(out), 0o755); err != nil {
		t.Fatalf("创建目录失败: %v", err)
	}
	if err := os.WriteFile(out, raw, 0o644); err != nil {
		t.Fatalf("写入失败: %v", err)
	}
	t.Logf("已导出 %d 字节到 %s", len(raw), filepath.ToSlash(out))
}
