package script

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"settings/internal/script/model"
)

// TestExportConfigSave 导出「Go 对某 config 输入 parse→save 的落盘字节」, 供 Rust 迁移端
// 逐字节比对 (跨语言锁死 map 键序)。
//
//	cd config-server && UPDATE_CONFIGSAVE_FIXTURE=1 go test ./internal/script/ -run TestExportConfigSave -count=1
//
// 为什么需要它: 落盘字节里含 **map 键序** —— Go `encoding/json` 对 map 键按字典序(UTF-8
// 字节序)输出, 而 Rust 若用 `HashMap` 则是随机迭代序 ⇒ 同一份配置两语言落盘字节必然不等。
// 该夹具把 Go 的真实字节冻结下来, 使 Rust 侧任何键序/转义/空集合口径的漂移都会立刻暴露。
//
// 口径: 两侧都只做「写入输入文件 -> ParseConfig(version=f 输入自带版本) -> 原子落盘」
// 这一条对称路径; `version` 随夹具下发, 避免 KeyfluxVersion 变量在两语言间取值不同。
func TestExportConfigSave(t *testing.T) {
	if os.Getenv("UPDATE_CONFIGSAVE_FIXTURE") != "1" {
		t.Skip("set UPDATE_CONFIGSAVE_FIXTURE=1 to export config-ui-reactor/tests/fixtures/config_save.json")
	}

	type saveCase struct {
		Name    string `json:"name"`
		Version string `json:"version"`
		Input   string `json:"input"`
		Saved   string `json:"saved"`
	}

	cases := []saveCase{
		{Name: "synthetic-hotkey-keys", Input: syntheticKeyOrderConfigJSON(t)},
	}
	// 真实大配置: 仓库出厂 data/config.json (16 个 keymap / 55KB) —— 覆盖真实 map 键集。
	if raw, err := os.ReadFile(filepath.Join("..", "..", "..", "data", "config.json")); err == nil {
		cases = append(cases, saveCase{Name: "factory", Input: string(raw)})
	} else {
		t.Logf("跳过 factory 用例 (读取仓库 data/config.json 失败: %v)", err)
	}

	out := make([]saveCase, 0, len(cases))
	for i := range cases {
		c := cases[i]
		c.Version = inputKeyfluxVersion(t, c.Input)
		c.Saved = goParseSaveBytes(t, c.Input, c.Version)
		out = append(out, c)
	}

	fixture := map[string]any{
		"version": "config-save/1",
		"cases":   out,
	}
	raw, err := json.MarshalIndent(fixture, "", "  ")
	if err != nil {
		t.Fatalf("序列化夹具失败: %v", err)
	}
	raw = append(raw, '\n')

	path := filepath.Join("..", "..", "..", "config-ui-reactor", "tests", "fixtures", "config_save.json")
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("创建夹具目录失败: %v", err)
	}
	if err := os.WriteFile(path, raw, 0o644); err != nil {
		t.Fatalf("写入夹具失败: %v", err)
	}
	for _, c := range out {
		t.Logf("用例 %-22s version=%-14q input=%d B -> saved=%d B", c.Name, c.Version, len(c.Input), len(c.Saved))
	}
	t.Logf("已导出 %d 个用例 / %d 字节到 %s", len(out), len(raw), filepath.ToSlash(path))
}

// syntheticKeyOrderConfigJSON 构造一份覆盖「字节序陷阱」的配置输入:
// 标点(*, / *. / *; / ,)、数字、大小写(A/Z vs a/z)、前缀关系(*z 与 A)、
// 长键 singlePress(大写 S 排在小写之后)、非 ASCII(网盘) 与 emoji(🔥key, 4 字节 UTF-8)。
func syntheticKeyOrderConfigJSON(t *testing.T) string {
	t.Helper()
	cfg := &Config{
		Keymaps: []Keymap{{
			ID:        5,
			Name:      "键序锁",
			Enable:    true,
			Hotkey:    "*CapsLock",
			ParentID:  0,
			Delay:     1000,
			DisableAt: "",
			Hotkeys: map[string][]Action{
				"*0": {{TypeID: 2, ValueID: 1}},
				"*,": {{TypeID: 6, KeysToSend: "{enter}"}},
				"*.": {{TypeID: 1, Target: "x.exe"}},
				"*;": {{TypeID: 9, ValueID: 6}},
				"*A": {{TypeID: 5, RemapToKey: "q"}},
				"*Z": {{TypeID: 5, RemapToKey: "w"}},
				"*a": {{TypeID: 7, ValueID: 1}},
				"*z": {{TypeID: 8, AHKCode: `MsgBox("hi")`}},
				",":  {{TypeID: 2, ValueID: 5}},
				"A":  {{TypeID: 6, KeysToSend: "{blind}A"}},
				"Z":  {{TypeID: 6, KeysToSend: "{blind}Z"}},
				"a":  {{TypeID: 3, ValueID: 1}},
				// 长键: 's'(0x73) 在 'a'..'z' 之间, 大写 'S' 使其排到小写字母之后
				"singlePress": {{TypeID: 9, ValueID: 8}},
				"z":           {{TypeID: 4, ValueID: 13}},
				// 非 ASCII: UTF-8 首字节 >= 0x80 ⇒ 恒排在全部 ASCII 键之后
				"网盘":   {{TypeID: 1, Target: "baidu.exe"}},
				"🔥key": {{TypeID: 1, Target: "fire.exe"}},
			},
		}},
		Options: Options{
			HideMatrix:     true,
			Language:       "zh-CN",
			KeyboardLayout: "",
			Mouse:          Mouse{TipSymbol: "🐶"},
			// 显式空切片 (而非缺省) —— 两侧「nil 语义差异」是**另一类**已知分歧
			// (Go nil slice -> `null`, Rust `Vec` -> `[]`), 会让本用例失焦。本用例
			// 只锁「键序 + 转义 + 缩进 + 尾换行」, 故把这三段非 map 字段钉成两侧同形。
			WindowGroups:  []WindowGroup{},
			PathVariables: []PathVariable{},
			Plugins:       PluginsOption{Disabled: []string{}},
		},
		SelectedAction: &model.SelectedAction{
			Hotkey: ">^p",
			Enable: true,
			Mappings: []model.SelectedMapping{{
				MatchType:  "textType",
				MatchValue: "url",
				Entries:    []model.SelectedEntry{{Behavior: "open_url"}},
			}},
		},
	}
	buf := new(bytes.Buffer)
	enc := json.NewEncoder(buf)
	enc.SetIndent("", "  ")
	enc.SetEscapeHTML(false)
	if err := enc.Encode(cfg); err != nil {
		t.Fatalf("构造合成输入失败: %v", err)
	}
	return buf.String()
}

// inputKeyfluxVersion 取输入自带的 options.keyfluxVersion (两侧 ParseConfig 都用它, 避免漂移)。
func inputKeyfluxVersion(t *testing.T, input string) string {
	t.Helper()
	var probe struct {
		Options struct {
			KeyfluxVersion string `json:"keyfluxVersion"`
		} `json:"options"`
	}
	if err := json.Unmarshal([]byte(input), &probe); err != nil {
		t.Fatalf("解析输入失败: %v", err)
	}
	return probe.Options.KeyfluxVersion
}

// goParseSaveBytes 复刻生产路径: 写输入 -> ParseConfig -> 原子落盘 -> 读回落盘字节。
func goParseSaveBytes(t *testing.T, input, version string) string {
	t.Helper()
	dir := t.TempDir()
	inPath := filepath.Join(dir, "config.json")
	if err := os.WriteFile(inPath, []byte(input), 0o644); err != nil {
		t.Fatalf("写输入失败: %v", err)
	}

	oldVersion := KeyfluxVersion
	KeyfluxVersion = version // ParseConfig 会用该值覆写 options.keyfluxVersion
	defer func() { KeyfluxVersion = oldVersion }()

	cfg, err := ParseConfig(inPath)
	if err != nil {
		t.Fatalf("ParseConfig 失败: %v", err)
	}
	outPath := filepath.Join(dir, "saved.json")
	if err := saveConfigFileTo(cfg, outPath); err != nil {
		t.Fatalf("落盘失败: %v", err)
	}
	saved, err := os.ReadFile(outPath)
	if err != nil {
		t.Fatalf("读回失败: %v", err)
	}
	return string(saved)
}
