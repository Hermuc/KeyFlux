package generators

import (
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
	inc, boot := pluginBlocks()
	if inc != "" || boot != "" {
		t.Fatalf("未注入目录时应为空块: inc=%q boot=%q", inc, boot)
	}
	SetPluginsDir(t.TempDir()) // 空目录
	inc, boot = pluginBlocks()
	if inc != "" || boot != "" {
		t.Fatalf("空目录应为空块: inc=%q boot=%q", inc, boot)
	}
	SetPluginsDir(filepath.Join(t.TempDir(), "不存在"))
	inc, boot = pluginBlocks()
	if inc != "" || boot != "" {
		t.Fatalf("缺失目录应为空块: inc=%q boot=%q", inc, boot)
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
	inc, boot := pluginBlocks()

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
	inc, boot := pluginBlocks()
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
		inc, boot := pluginBlocks()
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
	inc, boot := pluginBlocks()

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
	inc, boot := pluginBlocks()
	if inc != "" {
		t.Fatalf("非 script 入口不得产出 Include:\n%q", inc)
	}
	if !strings.Contains(boot, "[插件错误] decl") || !strings.Contains(boot, "仅支持 script") {
		t.Fatalf("缺校验错误注释:\n%q", boot)
	}
}
