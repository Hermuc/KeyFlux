package script

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"settings/internal/script/generators"
	"settings/internal/script/model"
)

// TestExportActionRender 导出「动作渲染器」的 Go 参考实现输出, 供 Rust 迁移端逐值对账 (P3)。
//
//	cd config-server && UPDATE_ACTION_FIXTURE=1 go test ./internal/script/ -run TestExportActionRender
//
// 为什么需要它: 动作渲染器跨端必须**逐字节**相同 —— Go `%t` 打印 bool、`ctx[2:]` / `line[6:]`
// 的字节切片、`renderKeymap` 的 CRLF 归一、`sortHotkeys` 的非稳定并列语义, 任一差异都会让
// KeyFlux.ahk 字节不等。手写期望值等于自证; 这里取 Go 的真实输出。
//
// 口径:
//   - 只经**导出入口**取值: `TemplateFuncMap["actionToHotkey"]` (= ActionToHotkey, inAbbrContext
//     恒 false) 与**导出的** `generators.ActionMap` (= map[int]func(model.Action, bool) string,
//     可取 inAbbrContext=true)。两者对 inAbbrContext=false 必须一致 (测试内断言)。
//   - Cfg 复用同包的 syntheticConfig(), 覆盖窗口组 1..5 / 键位映射 ID 5、6 / 全部 9 个 TypeID。
//   - `model.Action.RemapInHotIf` 是 `json:"-"` (不落盘) => 单靠 Action JSON 无法携带它;
//     故旁挂字段 `remapInHotIf`, Rust 端读回后赋值。
func TestExportActionRender(t *testing.T) {
	if os.Getenv("UPDATE_ACTION_FIXTURE") != "1" {
		t.Skip("set UPDATE_ACTION_FIXTURE=1 to export config-ui-reactor/tests/fixtures/action_render.json")
	}

	// Cfg 是包级全局; 取参考值前注入整份合成配置, 用完还原 (不污染同包其它测试)。
	oldCfg := generators.Cfg
	defer func() { generators.Cfg = oldCfg }()
	cfg := syntheticConfig()
	generators.Cfg = cfg

	actionToHotkey := generators.TemplateFuncMap["actionToHotkey"].(func(model.Action) string)
	abbrRegistry := generators.TemplateFuncMap["abbrRegistryCode"].(func(map[string][]model.Action, string, string) string)
	renderKeymap := generators.TemplateFuncMap["renderKeymap"].(func(model.Keymap) string)
	groupDisable := generators.TemplateFuncMap["GroupDisableKeyFlux"].(func([]model.WindowGroup) string)
	selectedCode := generators.TemplateFuncMap["selectedActionCode"].(func(*model.SelectedAction) string)

	type actionCase struct {
		In            model.Action `json:"in"`
		RemapInHotIf  bool         `json:"remapInHotIf"`
		InAbbrContext bool         `json:"inAbbrContext"`
		Out           string       `json:"out"`
	}
	type abbrCase struct {
		In     map[string][]model.Action `json:"in"`
		Scope  string                    `json:"scope"`
		Indent string                    `json:"indent"`
		Out    string                    `json:"out"`
	}
	type renderCase struct {
		In  model.Keymap `json:"in"`
		Out string       `json:"out"`
	}
	type groupCase struct {
		In  []model.WindowGroup `json:"in"`
		Out string              `json:"out"`
	}
	type selectedCase struct {
		In  *model.SelectedAction `json:"in"`
		Out string                `json:"out"`
	}

	// 每个 Action 覆盖 inAbbrContext 的两种取值; 未注册 TypeID 经 ActionMap 未命中也取空串。
	var cases []actionCase
	addCases := func(actions ...model.Action) {
		for _, a := range actions {
			for _, inAbbr := range []bool{false, true} {
				out := ""
				if fn, ok := generators.ActionMap[a.TypeID]; ok {
					out = fn(a, inAbbr)
				}
				if !inAbbr {
					// 导出入口 ActionToHotkey 恒传 false, 与 ActionMap(a,false) 必须一致
					if direct := actionToHotkey(a); direct != out {
						t.Fatalf("导出入口不一致: ActionToHotkey=%q ActionMap=%q action=%+v", direct, out, a)
					}
				}
				cases = append(cases, actionCase{In: a, RemapInHotIf: a.RemapInHotIf, InAbbrContext: inAbbr, Out: out})
			}
		}
	}

	addCases(
		// TypeID1: 简化形态 / 全参形态 / winTitle 警告 / 带窗口守卫
		model.Action{TypeID: 1, Hotkey: "*a", Target: "notepad.exe"},
		model.Action{TypeID: 1, Hotkey: "*b", Target: "calc.exe", Args: "/auto", WorkingDir: `D:\tools`, RunAsAdmin: true, DetectHiddenWindow: true, RunInBackground: true},
		model.Action{TypeID: 1, Hotkey: "*c", Target: "x.exe", WinTitle: "foo.exe", WindowGroupID: 1},
		model.Action{TypeID: 1, Hotkey: "*d", Target: "x.exe", WinTitle: "ahk_exe foo.exe", WindowGroupID: 2},
		// TypeID2: 边界(1) / 常规(5) / 末位(10) / 未注册值(11)
		model.Action{TypeID: 2, Hotkey: "*1", ValueID: 1},
		model.Action{TypeID: 2, Hotkey: "*2", ValueID: 5, WindowGroupID: 3},
		model.Action{TypeID: 2, Hotkey: "*3", ValueID: 10},
		model.Action{TypeID: 2, Hotkey: "*4", ValueID: 11},
		// TypeID3: 常规 / ValueID4 taskSwitch / ValueID14 BindWindow / 末位(16) / 未注册值
		model.Action{TypeID: 3, Hotkey: "*1", ValueID: 1},
		model.Action{TypeID: 3, Hotkey: "*2", ValueID: 4, WindowGroupID: 1},
		model.Action{TypeID: 3, Hotkey: "*3", ValueID: 14, WindowGroupID: 2},
		model.Action{TypeID: 3, Hotkey: "*4", ValueID: 16},
		model.Action{TypeID: 3, Hotkey: "*5", ValueID: 99},
		// TypeID4: 移动(带/不带守卫) / 滚轮(>=5) / 按键(9) / MoveMouseToCaret(13) / 未注册值
		model.Action{TypeID: 4, Hotkey: "*1", ValueID: 1},
		model.Action{TypeID: 4, Hotkey: "*2", ValueID: 2, WindowGroupID: 1},
		model.Action{TypeID: 4, Hotkey: "*3", ValueID: 5},
		model.Action{TypeID: 4, Hotkey: "*4", ValueID: 9, WindowGroupID: 2},
		model.Action{TypeID: 4, Hotkey: "*5", ValueID: 13},
		model.Action{TypeID: 4, Hotkey: "*6", ValueID: 99},
		// TypeID5: 普通 / 带守卫(ctx 去前导) / HotIf 分支 / singlePress(大小写) 退化发送
		model.Action{TypeID: 5, Hotkey: "*t", RemapToKey: "x"},
		model.Action{TypeID: 5, Hotkey: "*u", RemapToKey: "y", WindowGroupID: 1},
		model.Action{TypeID: 5, Hotkey: "*v", RemapToKey: "z", RemapInHotIf: true},
		model.Action{TypeID: 5, Hotkey: "singlePress", RemapToKey: "q"},
		model.Action{TypeID: 5, Hotkey: "SinglePress", RemapToKey: "q"},
		// TypeID6: 多行 Send / ahk:+sleep 混合 / 全空白
		model.Action{TypeID: 6, Hotkey: "*h", KeysToSend: "hello\n{text}world"},
		model.Action{TypeID: 6, Hotkey: "*i", KeysToSend: "ahk:ToolTip(\"hi\")\nsleep 500\n{enter}", WindowGroupID: 2},
		model.Action{TypeID: 6, Hotkey: "*j", KeysToSend: "   \n\n"},
		// TypeID7: remap(1/17) / send(7/33) / callMap(19) / 未注册值
		model.Action{TypeID: 7, Hotkey: "*1", ValueID: 1},
		model.Action{TypeID: 7, Hotkey: "*2", ValueID: 17, WindowGroupID: 1},
		model.Action{TypeID: 7, Hotkey: "*3", ValueID: 7},
		model.Action{TypeID: 7, Hotkey: "*4", ValueID: 33},
		model.Action{TypeID: 7, Hotkey: "*5", ValueID: 19},
		model.Action{TypeID: 7, Hotkey: "*6", ValueID: 99},
		// TypeID8: AHKCode 直出(带/不带守卫) / 空 AHKCode 退化 / 另一 conditionType 守卫
		model.Action{TypeID: 8, Hotkey: "*a", AHKCode: `MsgBox("hello")`},
		model.Action{TypeID: 8, Hotkey: "*b", AHKCode: `Run("calc.exe")`, WindowGroupID: 5},
		model.Action{TypeID: 8, Hotkey: "*c", AHKCode: ""},
		model.Action{TypeID: 8, Hotkey: "*d", AHKCode: "x := A_Now", WindowGroupID: 3},
		// TypeID9: 1/2(补 "S"+占位) / 5 / 6 / 8(ToggleLock) / 9 / 未注册值
		model.Action{TypeID: 9, Hotkey: "*1", ValueID: 1},
		model.Action{TypeID: 9, Hotkey: "*2", ValueID: 2, WindowGroupID: 1},
		model.Action{TypeID: 9, Hotkey: "*3", ValueID: 5},
		model.Action{TypeID: 9, Hotkey: "*4", ValueID: 6},
		model.Action{TypeID: 9, Hotkey: "*5", ValueID: 8},
		model.Action{TypeID: 9, Hotkey: "*6", ValueID: 9, WindowGroupID: 2},
		model.Action{TypeID: 9, Hotkey: "*7", ValueID: 99},
		// 未注册 TypeID
		model.Action{TypeID: 0, Hotkey: "*a"},
		model.Action{TypeID: 42, Hotkey: "*b"},
	)

	// AbbrRegistryCode: 无守卫 / ct1 / ct2(多行组) / ct5(去单引号) / 多动作 / 未注册跳过 / 空表
	abbrCases := []abbrCase{
		{
			In: map[string][]model.Action{
				"web":   {{TypeID: 1, Target: "chrome.exe"}},
				"jk":    {{TypeID: 6, KeysToSend: "{blind}jk"}},
				"expr":  {{TypeID: 8, AHKCode: `Run("calc.exe")`, WindowGroupID: 5}},
				"edit":  {{TypeID: 6, KeysToSend: "{blind}code", WindowGroupID: 1}},
				"multi": {{TypeID: 2, ValueID: 1}, {TypeID: 6, KeysToSend: "{enter}"}},
				"sys":   {{TypeID: 2, ValueID: 5, WindowGroupID: 2}},
				"bad":   {{TypeID: 0}},
			},
			Scope: "capslock", Indent: "  ",
		},
		{
			In:    map[string][]model.Action{"only": {{TypeID: 42}}},
			Scope: "semicolon", Indent: "  ",
		},
		{
			In:    map[string][]model.Action{},
			Scope: "capslock", Indent: "  ",
		},
	}
	for i := range abbrCases {
		abbrCases[i].Out = abbrRegistry(abbrCases[i].In, abbrCases[i].Scope, abbrCases[i].Indent)
	}

	// renderKeymap: 主模式(NewKeymap+disableAt 单行) / 子模式(AddSubKeymap) /
	// 纯修饰键(customHotkeys+singlePress 跳过) / 空白热键(空串)。
	renderCases := []renderCase{
		{
			In: model.Keymap{
				ID: 5, Name: "CapsLock", Enable: true, Hotkey: "*CapsLock", ParentID: 0, Delay: 1000,
				Hotkeys: map[string][]model.Action{
					"*1": {{TypeID: 2, ValueID: 1}},
					"*2": {{TypeID: 6, KeysToSend: "{enter}"}},
				},
			},
		},
		{
			In: model.Keymap{
				ID: 6, Name: "媒体控制", Enable: true, Hotkey: "*F13", ParentID: 5, Delay: 0,
				Hotkeys: map[string][]model.Action{
					"m6": {{TypeID: 9, ValueID: 4}},
				},
			},
		},
		{
			In: model.Keymap{
				ID: 7, Name: "Mods", Enable: true, Hotkey: "#!^", ParentID: 0, Delay: 500,
				Hotkeys: map[string][]model.Action{
					"q":           {{TypeID: 2, ValueID: 1}},
					"singlePress": {{TypeID: 2, ValueID: 2}},
					"w":           {{TypeID: 6, KeysToSend: "{enter}"}},
				},
			},
		},
		{
			In: model.Keymap{ID: 8, Name: "Blank", Enable: true, Hotkey: "   "},
		},
	}
	for i := range renderCases {
		renderCases[i].Out = renderKeymap(renderCases[i].In)
	}

	// GroupDisableKeyFlux: 命中 ID=-1(单行/多行) / 未命中 / nil slice
	groupCases := []groupCase{
		{In: []model.WindowGroup{{ID: -1, Value: "steam.exe"}, {ID: 1, Value: "x.exe"}}},
		{In: []model.WindowGroup{{ID: 1, Value: "x.exe"}, {ID: -1, Value: "a.exe\nb.exe"}}},
		{In: []model.WindowGroup{{ID: 1, Value: "x.exe"}}},
		{In: nil},
	}
	for i := range groupCases {
		groupCases[i].Out = groupDisable(groupCases[i].In)
	}

	// selectedActionCode: nil / 禁用 / 空热键 / 完整(含超 key cap 的 entry)
	selCases := []selectedCase{
		{In: nil},
		{In: &model.SelectedAction{Enable: false, Hotkey: ">^p"}},
		{In: &model.SelectedAction{Enable: true, Hotkey: ""}},
		{
			In: &model.SelectedAction{
				Hotkey: ">^p", Enable: true,
				Mappings: []model.SelectedMapping{
					{
						MatchType: "textType", MatchValue: "url",
						Entries: []model.SelectedEntry{
							{Behavior: "open_url"},
							{Behavior: "search", ActionValue: "https://www.bing.com/search?q=%selected%"},
						},
					},
					{
						MatchType: "fileExt", MatchValue: "jpg,png",
						Entries: []model.SelectedEntry{
							{Behavior: "open", ActionValue: "%selected%"},
							{Behavior: "e1"}, {Behavior: "e2"}, {Behavior: "e3"}, {Behavior: "e4"},
							{Behavior: "e5"}, {Behavior: "e6"}, {Behavior: "e7"}, {Behavior: "e8"},
							{Behavior: "e9"}, {Behavior: "e10"},
						},
					},
				},
			},
		},
	}
	for i := range selCases {
		selCases[i].Out = selectedCode(selCases[i].In)
	}

	fixture := map[string]any{
		"config":              cfg,
		"cases":               cases,
		"abbrRegistry":        abbrCases,
		"renderKeymap":        renderCases,
		"groupDisableKeyFlux": groupCases,
		"selectedAction":      selCases,
	}

	raw, err := json.MarshalIndent(fixture, "", "  ")
	if err != nil {
		t.Fatalf("序列化失败: %v", err)
	}
	raw = append(raw, '\n')

	out := filepath.Join("..", "..", "..", "config-ui-reactor", "tests", "fixtures", "action_render.json")
	if err := os.MkdirAll(filepath.Dir(out), 0o755); err != nil {
		t.Fatalf("创建目录失败: %v", err)
	}
	if err := os.WriteFile(out, raw, 0o644); err != nil {
		t.Fatalf("写入失败: %v", err)
	}
	t.Logf("已导出动作用例 %d 条 / 缩写 %d 条 / renderKeymap %d 条 / 窗口组 %d 条 / 选中动作 %d 条, %d 字节到 %s",
		len(cases), len(abbrCases), len(renderCases), len(groupCases), len(selCases), len(raw), filepath.ToSlash(out))
}
