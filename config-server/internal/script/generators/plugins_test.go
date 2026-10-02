package generators

import (
	"fmt"
	"os"
	"path/filepath"
	"settings/internal/script/model"
	"strings"
	"testing"
)

// 零插件 (目录为空/缺失/未注入) 时两块必须为空串 —— 模板行尾拼接约定下,
// 生成产物与历史字节一致 (不破坏现有 Oracle/golden 基线)。
func TestRenderPluginBlocks_Empty(t *testing.T) {
	SetPluginsDir("")
	inc, boot, late := pluginBlocks()
	if inc != "" || boot != "" || late != "" {
		t.Fatalf("未注入目录时应为空块: inc=%q boot=%q late=%q", inc, boot, late)
	}
	SetPluginsDir(t.TempDir()) // 空目录
	inc, boot, late = pluginBlocks()
	if inc != "" || boot != "" || late != "" {
		t.Fatalf("空目录应为空块: inc=%q boot=%q late=%q", inc, boot, late)
	}
	SetPluginsDir(filepath.Join(t.TempDir(), "不存在"))
	inc, boot, late = pluginBlocks()
	if inc != "" || boot != "" || late != "" {
		t.Fatalf("缺失目录应为空块: inc=%q boot=%q late=%q", inc, boot, late)
	}
}

// 正常 script 插件: 产出 #Include 行 + Register/LoadEntry 引导行;
// manifest 渲染为 AHK Map 字面量 (含嵌套 entry Map 与 permissions 数组)。
func TestRenderPluginBlocks_ScriptPlugin(t *testing.T) {
	dir := t.TempDir()
	pdir := filepath.Join(dir, "sample_greeter")
	os.MkdirAll(pdir, 0o755)
	os.WriteFile(filepath.Join(pdir, "plugin.json"), []byte(`{
		"id": "sample_greeter", "name": "示例问候", "nameEn": "Sample Greeter",
		"version": "1.0", "specVersion": 1, "description": "演示用",
		"entry": {"kind": "script", "file": "main.ahk", "func": "SampleGreeterMain"},
		"permissions": ["run", "selection"]
	}`), 0o644)
	os.WriteFile(filepath.Join(pdir, "main.ahk"), []byte("SampleGreeterMain(api) {}"), 0o644)

	SetPluginsDir(dir)
	inc, boot, late := pluginBlocks()

	if inc != "\n#Include ../data/plugins/sample_greeter/main.ahk" {
		t.Fatalf("Include 块不符:\n%q", inc)
	}
	if !strings.Contains(boot, "\nPluginManager.Register(Map(\"id\", \"sample_greeter\"") ||
		!strings.Contains(boot, `"entry", Map("kind", "script", "file", "main.ahk", "func", "SampleGreeterMain")`) ||
		!strings.Contains(boot, `"permissions", ["run", "selection"]`) {
		t.Fatalf("Register 渲染不符:\n%q", boot)
	}
	if !strings.Contains(boot, "\nPluginManager.LoadEntry(\"sample_greeter\")") {
		t.Fatalf("缺 LoadEntry:\n%q", boot)
	}
	// 字符串转义: 不允许裸换行/裸反引号泄漏进字面量
	if strings.Contains(boot, "\n\", ") || strings.Count(boot, "`")%2 != 0 {
		t.Fatalf("字面量转义异常:\n%q", boot)
	}
	// 非 quick_switch 插件不得产出晚初始化行 (晚初始化扩展点当前唯一消费方)
	if late != "" {
		t.Fatalf("非 quick_switch 不得产出晚初始化行:\n%q", late)
	}
}

// 入口文件缺失: 生成期拦截 (AHK #Include 指向缺失文件会拖垮整个脚本加载),
// 插件跳过并落注释警告。
func TestRenderPluginBlocks_MissingEntryFile(t *testing.T) {
	dir := t.TempDir()
	pdir := filepath.Join(dir, "broken")
	os.MkdirAll(pdir, 0o755)
	os.WriteFile(filepath.Join(pdir, "plugin.json"), []byte(`{
		"id": "broken", "name": "坏包", "specVersion": 1,
		"entry": {"kind": "script", "file": "main.ahk", "func": "Register"}
	}`), 0o644)

	SetPluginsDir(dir)
	inc, boot, _ := pluginBlocks()
	if inc != "" {
		t.Fatalf("入口缺失时不得产出 Include:\n%q", inc)
	}
	if !strings.Contains(boot, "[插件警告] broken: 入口文件缺失") {
		t.Fatalf("缺警告注释:\n%q", boot)
	}
	if strings.Contains(boot, "PluginManager.Register(") {
		t.Fatalf("坏包不得注册:\n%q", boot)
	}
}

// entry.file 路径逃逸 (.. 段 / 绝对路径 / 盘符) 必须拒绝。
func TestRenderPluginBlocks_PathEscape(t *testing.T) {
	for _, evil := range []string{`../evil.ahk`, `sub/../main.ahk`, `/abs/main.ahk`, `C:/x/main.ahk`, `..\\evil.ahk`} {
		dir := t.TempDir()
		pdir := filepath.Join(dir, "evil")
		os.MkdirAll(pdir, 0o755)
		os.WriteFile(filepath.Join(pdir, "plugin.json"), []byte(`{
			"id": "evil", "name": "evil", "specVersion": 1,
			"entry": {"kind": "script", "file": "`+evil+`", "func": "Register"}
		}`), 0o644)

		SetPluginsDir(dir)
		inc, boot, _ := pluginBlocks()
		if inc != "" {
			t.Fatalf("路径逃逸 %q 不得产出 Include:\n%q", evil, inc)
		}
		if !strings.Contains(boot, "entry.file 非法") && !strings.Contains(boot, "[插件错误] evil") {
			t.Fatalf("路径逃逸 %q 缺拦截:\n%q", evil, boot)
		}
	}
}

// 停用持久化 (config.options.plugins.disabled): 停用插件不注入不注册 (落注释),
// 启用插件不受影响 —— 与设置面板开关写配置链路 (OnCardEnabledChanged→SaveAsync) 闭环。
func TestRenderPluginBlocks_Disabled(t *testing.T) {
	dir := t.TempDir()
	for _, id := range []string{"on", "off"} {
		pdir := filepath.Join(dir, id)
		os.MkdirAll(pdir, 0o755)
		os.WriteFile(filepath.Join(pdir, "plugin.json"), []byte(`{
			"id": "`+id+`", "name": "`+id+`", "specVersion": 1,
			"entry": {"kind": "script", "file": "main.ahk", "func": "Register"}
		}`), 0o644)
		os.WriteFile(filepath.Join(pdir, "main.ahk"), []byte("Register(api) {}"), 0o644)
	}

	Cfg = &model.Config{Options: model.Options{Plugins: model.PluginsOption{Disabled: []string{"off"}}}}
	defer func() { Cfg = nil }()
	SetPluginsDir(dir)
	inc, boot, _ := pluginBlocks()

	if strings.Contains(inc, "/off/") {
		t.Fatalf("停用插件不得产出 Include:\n%q", inc)
	}
	if !strings.Contains(inc, "/on/main.ahk") {
		t.Fatalf("启用插件应正常注入:\n%q", inc)
	}
	if !strings.Contains(boot, "[插件] off 已在配置中停用") {
		t.Fatalf("缺停用注释:\n%q", boot)
	}
	if strings.Contains(boot, "PluginManager.Register(Map(\"id\", \"off\"") {
		t.Fatalf("停用插件不得注册:\n%q", boot)
	}
	if !strings.Contains(boot, "PluginManager.Register(Map(\"id\", \"on\"") {
		t.Fatalf("启用插件应注册:\n%q", boot)
	}
}

// 非 script 入口: ValidateManifest 在 LoadCatalog 阶段即拒绝 (当前仅支持 script),
// 经 Catalog.Errors 渲染为 [插件错误] 注释; renderPluginBlocks 的 kind 守卫为纵深防御。
func TestRenderPluginBlocks_NonScriptEntry(t *testing.T) {
	dir := t.TempDir()
	pdir := filepath.Join(dir, "decl")
	os.MkdirAll(pdir, 0o755)
	os.WriteFile(filepath.Join(pdir, "plugin.json"), []byte(`{
		"id": "decl", "name": "声明式", "specVersion": 1,
		"entry": {"kind": "builtin", "action": "run"}
	}`), 0o644)

	SetPluginsDir(dir)
	inc, boot, _ := pluginBlocks()
	if inc != "" {
		t.Fatalf("非 script 入口不得产出 Include:\n%q", inc)
	}
	if !strings.Contains(boot, "[插件错误] decl") || !strings.Contains(boot, "仅支持 script") {
		t.Fatalf("缺校验错误注释:\n%q", boot)
	}
}

// quick_switch 晚初始化行: 插件存在时产出 InitQuickSwitch(...) 调用,
// 字节形态必须与迁移前模板硬编码行完全一致 (字段顺序/分隔符/bool 文本/转义)。
func TestPluginLateInit_QuickSwitch(t *testing.T) {
	dir := t.TempDir()
	pdir := filepath.Join(dir, "quick_switch")
	os.MkdirAll(pdir, 0o755)
	os.WriteFile(filepath.Join(pdir, "plugin.json"), []byte(`{
		"id": "quick_switch", "name": "快速切换", "nameEn": "Quick Switch",
		"version": "1.0.0", "specVersion": 1, "description": "内置插件化",
		"entry": {"kind": "script", "file": "main.ahk", "func": "QuickSwitchMain"},
		"permissions": ["window"]
	}`), 0o644)
	os.WriteFile(filepath.Join(pdir, "main.ahk"), []byte("QuickSwitchMain(api) {}"), 0o644)

	// 注入配置: Cfg 为 generators 包的全局 (SaveAHK 在模板执行前注入)
	oldCfg := Cfg
	Cfg = &model.Config{}
	Cfg.Options.QuickSwitch = model.QuickSwitchOption{
		CollectEnabled: true, AutoShow: true, AutoJumpOpen: true, AutoJumpSave: false,
		PollIntervalMs: 800, MaxHistory: 20, OverlayRows: 8, OverlayRowsCompact: 4,
		ExcludedPrefixes: []string{"C:\\Temp", "D:\\「引号」"},
	}
	defer func() { Cfg = oldCfg }()

	SetPluginsDir(dir)
	_, boot, late := pluginBlocks()

	want := "\nInitQuickSwitch({collectEnabled: true, autoShow: true, autoJumpOpen: true, autoJumpSave: false, pollIntervalMs: 800, maxHistory: 20, overlayRows: 8, overlayRowsCompact: 4, excludedPrefixes: [\"C:\\Temp\", \"D:\\「引号」\"]})"
	if late != want {
		t.Fatalf("晚初始化行字节不符:\n got=%q\nwant=%q", late, want)
	}
	// 引导行照常产出 (真插件身份)
	if !strings.Contains(boot, "\nPluginManager.LoadEntry(\"quick_switch\")") {
		t.Fatalf("quick_switch 应注册引导行:\n%q", boot)
	}
}

// quick_switch 被禁用/入口缺失时晚初始化行不得产出 (可删除性保证: AHK v2 直调
// 未定义函数是加载期致命错误, 删除插件后 InitQuickSwitch 符号不存在)。
func TestPluginLateInit_QuickSwitchAbsent(t *testing.T) {
	oldCfg := Cfg
	Cfg = &model.Config{}
	Cfg.Options.QuickSwitch = model.QuickSwitchOption{CollectEnabled: true}
	defer func() { Cfg = oldCfg }()

	// 入口缺失
	dir := t.TempDir()
	pdir := filepath.Join(dir, "quick_switch")
	os.MkdirAll(pdir, 0o755)
	os.WriteFile(filepath.Join(pdir, "plugin.json"), []byte(`{
		"id": "quick_switch", "name": "快速切换", "specVersion": 1,
		"entry": {"kind": "script", "file": "main.ahk", "func": "QuickSwitchMain"}
	}`), 0o644)
	SetPluginsDir(dir)
	_, boot, late := pluginBlocks()
	if late != "" {
		t.Fatalf("入口缺失时不得产出晚初始化行:\n%q", late)
	}
	if !strings.Contains(boot, "入口文件缺失") {
		t.Fatalf("缺入口缺失警告:\n%q", boot)
	}

	// 配置禁用 (options.plugins.disabled)
	dir2 := t.TempDir()
	pdir2 := filepath.Join(dir2, "quick_switch")
	os.MkdirAll(pdir2, 0o755)
	os.WriteFile(filepath.Join(pdir2, "plugin.json"), []byte(`{
		"id": "quick_switch", "name": "快速切换", "specVersion": 1,
		"entry": {"kind": "script", "file": "main.ahk", "func": "QuickSwitchMain"}
	}`), 0o644)
	os.WriteFile(filepath.Join(pdir2, "main.ahk"), []byte("QuickSwitchMain(api) {}"), 0o644)
	oldDisabled := Cfg.Options.Plugins.Disabled
	Cfg.Options.Plugins.Disabled = []string{"quick_switch"}
	defer func() { Cfg.Options.Plugins.Disabled = oldDisabled }()
	SetPluginsDir(dir2)
	_, boot2, late2 := pluginBlocks()
	if late2 != "" {
		t.Fatalf("禁用插件不得产出晚初始化行:\n%q", late2)
	}
	if !strings.Contains(boot2, "已在配置中停用") {
		t.Fatalf("缺停用注释:\n%q", boot2)
	}
}

// 墓碑 (2026-10-02 P4): options.plugins.removed 中的随包内置插件不注入、只产注释行
// —— 目录被 sync-plugins 带回时不复活。文案与 Rust 同构 (parity 产物一致)。
func TestPluginTombstone_SkipsInjection(t *testing.T) {
	oldCfg := Cfg
	Cfg = &model.Config{}
	Cfg.Options.Plugins.Removed = []string{"quick_switch"}
	defer func() { Cfg = oldCfg }()

	pluginDir := t.TempDir()
	for _, id := range []string{"quick_switch", "everything_search"} {
		pdir := filepath.Join(pluginDir, id)
		os.MkdirAll(pdir, 0o755)
		os.WriteFile(filepath.Join(pdir, "plugin.json"), []byte(fmt.Sprintf(
			`{"id": %q, "name": "X", "specVersion": 1, "entry": {"kind": "script", "file": "main.ahk", "func": "F"}}`, id)), 0o644)
		os.WriteFile(filepath.Join(pdir, "main.ahk"), []byte("F(api) {}"), 0o644)
	}
	SetPluginsDir(pluginDir)
	defer SetPluginsDir("")

	inc, boot, late := pluginBlocks()
	if strings.Contains(inc, "quick_switch") {
		t.Fatalf("墓碑插件不得注入 include:\n%q", inc)
	}
	if !strings.Contains(boot, "\n; [插件] quick_switch 已被用户移除 (墓碑), 跳过加载") {
		t.Fatalf("缺墓碑注释:\n%q", boot)
	}
	if !strings.Contains(boot, "PluginManager.LoadEntry(\"everything_search\")") {
		t.Fatalf("其他插件应照常注入:\n%q", boot)
	}
	if strings.Contains(late, "InitQuickSwitch") {
		t.Fatalf("墓碑插件晚初始化行不得产出:\n%q", late)
	}
}
