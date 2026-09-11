package server

import (
	"encoding/json"
	"testing"
)

// TestNilSliceRoundTrip 验证 Hotkeys map 中 value 为 null 时,
// DTOToConfig→ConfigToDTO 往返仍保持 null (非 []), 保证 wire 恒等。
func TestNilSliceRoundTrip(t *testing.T) {
	// 模拟前端 PUT 的 JSON: hotkeys 里某个 key 对应 null (无动作绑定)
	input := `{"keymaps":[{"id":1,"name":"test","enable":true,"hotkey":"","parentID":0,"delay":0,"disableAt":"","hotkeys":{"x":null}}]}`

	var dto ConfigDTO
	if err := json.Unmarshal([]byte(input), &dto); err != nil {
		t.Fatalf("反序列化 DTO 失败: %v", err)
	}

	// DTO → model
	cfg := DTOToConfig(&dto)

	// 验证 model 侧: Hotkeys["x"] 应为 nil
	if cfg.Keymaps[0].Hotkeys["x"] != nil {
		t.Errorf("DTOToConfig: Hotkeys[\"x\"] 应为 nil, 实际为 %v", cfg.Keymaps[0].Hotkeys["x"])
	}

	// model → DTO
	dto2 := ConfigToDTO(cfg)

	// 验证 DTO 侧: Hotkeys["x"] 应为 nil
	if dto2.Keymaps[0].Hotkeys["x"] != nil {
		t.Errorf("ConfigToDTO: Hotkeys[\"x\"] 应为 nil, 实际为 %v", dto2.Keymaps[0].Hotkeys["x"])
	}

	// 最终序列化验证: JSON 中 "x" 的值应为 null 而非 []
	out, err := json.Marshal(dto2)
	if err != nil {
		t.Fatalf("序列化失败: %v", err)
	}
	var check map[string]interface{}
	if err := json.Unmarshal(out, &check); err != nil {
		t.Fatalf("反序列化验证失败: %v", err)
	}
	keymaps := check["keymaps"].([]interface{})
	km := keymaps[0].(map[string]interface{})
	hotkeys := km["hotkeys"].(map[string]interface{})
	if hotkeys["x"] != nil {
		t.Errorf("wire JSON: hotkeys.x 应为 null, 实际为 %v", hotkeys["x"])
	}
}

// TestSelectedActionRoundTrip 验证 selectedAction 的 PUT 反序列化 + DTOToConfig →
// ConfigToDTO 往返: 字段值逐项一致, mappings/entries 空集合恒输出 [],
// 序列化后 selectedAction 子树与 PUT 输入字节级一致 (GET/PUT wire 契约,
// 镜像 C# ConfigContractTests 模式)。
func TestSelectedActionRoundTrip(t *testing.T) {
	input := `{"keymaps":[],"selectedAction":{"hotkey":">^p","enable":true,"mappings":[
		{"matchType":"textType","matchValue":"url","entries":[
			{"behavior":"open_url","options":{"copyToClipboard":false,"clearSelection":false,"confirm":false}},
			{"behavior":"search","actionValue":"https://www.bing.com/search?q=%selected%","options":{"copyToClipboard":true,"clearSelection":true,"confirm":true}}
		]},
		{"matchType":"fileExt","matchValue":"jpg,png","entries":[
			{"behavior":"open","actionValue":"%selected%","workingDir":"D:\\tools","options":{"copyToClipboard":true,"clearSelection":false,"confirm":false}}
		]}
	]}}`

	var dto ConfigDTO
	if err := json.Unmarshal([]byte(input), &dto); err != nil {
		t.Fatalf("反序列化 DTO 失败: %v", err)
	}

	cfg := DTOToConfig(&dto)
	if cfg.SelectedAction == nil {
		t.Fatal("DTOToConfig: SelectedAction 不应为 nil")
	}
	if cfg.SelectedAction.Hotkey != ">^p" || !cfg.SelectedAction.Enable {
		t.Errorf("DTOToConfig: hotkey/enable 往返丢失: %+v", cfg.SelectedAction)
	}
	if len(cfg.SelectedAction.Mappings) != 2 {
		t.Fatalf("DTOToConfig: mappings 应有 2 项, 实际 %d", len(cfg.SelectedAction.Mappings))
	}
	if len(cfg.SelectedAction.Mappings[0].Entries) != 2 {
		t.Fatalf("DTOToConfig: entries 应有 2 项, 实际 %d", len(cfg.SelectedAction.Mappings[0].Entries))
	}
	e := cfg.SelectedAction.Mappings[0].Entries[1]
	if e.Behavior != "search" || e.ActionValue != "https://www.bing.com/search?q=%selected%" ||
		!e.Options.CopyToClipboard || !e.Options.ClearSelection || !e.Options.Confirm {
		t.Errorf("DTOToConfig: entry 字段往返不一致: %+v", e)
	}
	if wd := cfg.SelectedAction.Mappings[1].Entries[0].WorkingDir; wd != `D:\tools` {
		t.Errorf("DTOToConfig: workingDir 反斜杠往返不一致: %q", wd)
	}

	// model → DTO → wire JSON, 逐层验证结构契约
	dto2 := ConfigToDTO(cfg)
	out, err := json.Marshal(dto2)
	if err != nil {
		t.Fatalf("序列化失败: %v", err)
	}
	var check map[string]interface{}
	if err := json.Unmarshal(out, &check); err != nil {
		t.Fatalf("反序列化验证失败: %v", err)
	}
	sa, ok := check["selectedAction"].(map[string]interface{})
	if !ok {
		t.Fatalf("wire JSON: selectedAction 应为对象且恒存在, 实际 %v", check["selectedAction"])
	}
	if sa["hotkey"] != ">^p" {
		t.Errorf("wire JSON: hotkey 应为 >^p, 实际 %v", sa["hotkey"])
	}
	ms, ok := sa["mappings"].([]interface{})
	if !ok || len(ms) != 2 {
		t.Fatalf("wire JSON: mappings 应为数组且 2 项, 实际 %v", sa["mappings"])
	}
	m0 := ms[0].(map[string]interface{})
	if m0["matchValue"] != "url" {
		t.Errorf("wire JSON: mappings[0].matchValue 应为 url, 实际 %v", m0["matchValue"])
	}
	es, ok := m0["entries"].([]interface{})
	if !ok || len(es) != 2 {
		t.Fatalf("wire JSON: entries 应为数组且 2 项, 实际 %v", m0["entries"])
	}
	e0 := es[0].(map[string]interface{})
	if _, has := e0["actionValue"]; has {
		t.Errorf("wire JSON: 空 actionValue 应 omitempty 缺键, 实际 %v", e0["actionValue"])
	}

	// 字节级契约: selectedAction 子树序列化后与 PUT 输入的子树一致
	// (两侧都经 map[string]interface{} 归一化, 消除键序差异后按字节比较)
	var reIn map[string]interface{}
	if err := json.Unmarshal([]byte(input), &reIn); err != nil {
		t.Fatal(err)
	}
	want, _ := json.Marshal(reIn["selectedAction"])
	got, _ := json.Marshal(sa)
	if string(want) != string(got) {
		t.Errorf("selectedAction 子树往返字节级不一致:\n want %s\n got  %s", want, got)
	}

	// 空集合恒数组契约: 出厂默认 (空 selectedAction) 时 mappings 恒输出 [] 而非 null/缺键
	var emptyIn ConfigDTO
	if err := json.Unmarshal([]byte(`{}`), &emptyIn); err != nil {
		t.Fatal(err)
	}
	out2, err := json.Marshal(ConfigToDTO(DTOToConfig(&emptyIn)))
	if err != nil {
		t.Fatal(err)
	}
	var emptyCheck map[string]interface{}
	if err := json.Unmarshal(out2, &emptyCheck); err != nil {
		t.Fatal(err)
	}
	sa2, ok := emptyCheck["selectedAction"].(map[string]interface{})
	if !ok {
		t.Fatalf("wire JSON: selectedAction 恒为对象, 实际 %v", emptyCheck["selectedAction"])
	}
	ms2, ok := sa2["mappings"].([]interface{})
	if !ok || len(ms2) != 0 {
		t.Errorf("空 selectedAction 的 mappings 应为 [], 实际 %v", sa2["mappings"])
	}
}

// TestPluginsRegistryRoundTrip 验证 options.plugins 注册表 (disabled 列表) 的
// PUT 反序列化 + DTOToConfig → ConfigToDTO 往返, 以及空集合恒数组契约
// (旧配置缺失该段 -> null -> [], 镜像 mappings 的 wire 契约)。
func TestPluginsRegistryRoundTrip(t *testing.T) {
	input := `{"keymaps":[],"options":{"plugins":{"disabled":["a_plugin"]}}}`
	var dto ConfigDTO
	if err := json.Unmarshal([]byte(input), &dto); err != nil {
		t.Fatalf("反序列化 DTO 失败: %v", err)
	}
	if len(dto.Options.Plugins.Disabled) != 1 || dto.Options.Plugins.Disabled[0] != "a_plugin" {
		t.Fatalf("PUT plugins.disabled 往返丢失: %+v", dto.Options.Plugins)
	}
	cfg := DTOToConfig(&dto)
	if len(cfg.Options.Plugins.Disabled) != 1 || cfg.Options.Plugins.Disabled[0] != "a_plugin" {
		t.Fatalf("DTOToConfig plugins.disabled 往返丢失: %+v", cfg.Options.Plugins)
	}
	dto2 := ConfigToDTO(cfg)
	out, err := json.Marshal(dto2)
	if err != nil {
		t.Fatal(err)
	}
	var check map[string]interface{}
	if err := json.Unmarshal(out, &check); err != nil {
		t.Fatal(err)
	}
	plugins := check["options"].(map[string]interface{})["plugins"].(map[string]interface{})
	disabled, ok := plugins["disabled"].([]interface{})
	if !ok || len(disabled) != 1 || disabled[0] != "a_plugin" {
		t.Fatalf("wire plugins.disabled 应为数组且值保留, 实际 %v", plugins["disabled"])
	}

	// 空注册表恒数组
	var emptyIn ConfigDTO
	if err := json.Unmarshal([]byte(`{}`), &emptyIn); err != nil {
		t.Fatal(err)
	}
	out2, err := json.Marshal(ConfigToDTO(DTOToConfig(&emptyIn)))
	if err != nil {
		t.Fatal(err)
	}
	var emptyCheck map[string]interface{}
	if err := json.Unmarshal(out2, &emptyCheck); err != nil {
		t.Fatal(err)
	}
	emptyPlugins, ok := emptyCheck["options"].(map[string]interface{})["plugins"].(map[string]interface{})
	if !ok {
		t.Fatalf("wire JSON: options.plugins 恒为对象, 实际 %v", emptyCheck["options"])
	}
	if _, ok := emptyPlugins["disabled"].([]interface{}); !ok {
		t.Fatalf("空 plugins.disabled 应为 [], 实际 %v", emptyPlugins["disabled"])
	}
}
