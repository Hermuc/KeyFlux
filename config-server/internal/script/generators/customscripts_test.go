package generators

import (
	"os"
	"path/filepath"
	"settings/internal/script/model"
	"strings"
	"testing"
)

// writeCustomScript 在 dir/scripts/ 下落一个外接脚本 (内容即文件体)。
func writeCustomScript(t *testing.T, dir, name, content string) {
	t.Helper()
	sdir := filepath.Join(dir, "scripts")
	if err := os.MkdirAll(sdir, 0o755); err != nil {
		t.Fatalf("建 scripts 目录失败: %v", err)
	}
	if err := os.WriteFile(filepath.Join(sdir, name), []byte(content), 0o644); err != nil {
		t.Fatalf("写 %s 失败: %v", name, err)
	}
}

// 零外接脚本 (目录未注入 / scripts 缺失 / 目录为空) 时三块必须为空串 —— 模板行尾拼接
// 约定下, 生成产物与历史字节一致 (不破坏现有 golden/parity 基线)。
func TestCustomScriptBlocks_Empty(t *testing.T) {
	SetCustomScriptsDir("")
	inc, boot, fn := customBlocksOnce()
	if inc != "" || boot != "" || fn != "" {
		t.Fatalf("未注入目录时应为空块: inc=%q boot=%q fn=%q", inc, boot, fn)
	}
	SetCustomScriptsDir(t.TempDir()) // 无 scripts/ 子目录
	inc, boot, fn = customBlocksOnce()
	if inc != "" || boot != "" || fn != "" {
		t.Fatalf("无 scripts 目录应为空块: inc=%q boot=%q fn=%q", inc, boot, fn)
	}
	SetCustomScriptsDir(filepath.Join(t.TempDir(), "不存在"))
	inc, boot, fn = customBlocksOnce()
	if inc != "" || boot != "" || fn != "" {
		t.Fatalf("缺失目录应为空块: inc=%q boot=%q fn=%q", inc, boot, fn)
	}
}

// 导出 Register(api) 的脚本: 产出 #Include + 合成清单 Register/LoadEntry 引导 +
// CustomScriptRun 名录分发 (挂接 ScriptHost)。
func TestCustomScriptBlocks_EntryScript(t *testing.T) {
	dir := t.TempDir()
	writeCustomScript(t, dir, "backup_task.ahk", "Register(api) {\n  api.run.RunScript(\"x\")\n}")

	SetCustomScriptsDir(dir)
	inc, boot, fn := customBlocksOnce()

	if inc != "\n#Include ../data/scripts/backup_task.ahk" {
		t.Fatalf("Include 块不符:\n%q", inc)
	}
	if !strings.Contains(boot, "\nPluginManager.Register(Map(\"id\", \"custom:backup_task\"") ||
		!strings.Contains(boot, `"entry", Map("kind", "script", "file", "scripts/backup_task.ahk", "func", "Register")`) ||
		!strings.Contains(boot, `"permissions", ["selection", "run", "clipboard", "window", "settings", "events"]`) {
		t.Fatalf("Register 渲染不符:\n%q", boot)
	}
	if !strings.Contains(boot, "\nPluginManager.LoadEntry(\"custom:backup_task\")") {
		t.Fatalf("缺 LoadEntry:\n%q", boot)
	}
	// 名录分发: 挂接 ScriptHost (约束 4), 表内是数据引用 (约束 7)
	if !strings.Contains(fn, "CustomScriptRun(id, args := \"\") {") ||
		!strings.Contains(fn, `"backup_task", A_ScriptDir "\..\data\scripts\backup_task.ahk"`) ||
		!strings.Contains(fn, "ScriptHost.Run(A_ScriptDir \"\\AutoHotkey64.exe\"") {
		t.Fatalf("CustomScriptRun 渲染不符:\n%q", fn)
	}
	// 字面量转义: 不允许裸换行/裸反引号泄漏
	if strings.Contains(boot, "\n\", ") || strings.Count(boot, "`")%2 != 0 || strings.Count(fn, "`")%2 != 0 {
		t.Fatalf("字面量转义异常:\nboot=%q\nfn=%q", boot, fn)
	}
}

// 未导出 Register(api) 的脚本: 仅函数库挂载 (等价 custom_functions.ahk 既有用法),
// 不产 Register/LoadEntry (避免运行时 plugin_error 噪声), 但名录分发仍收录。
func TestCustomScriptBlocks_LibraryScript(t *testing.T) {
	dir := t.TempDir()
	writeCustomScript(t, dir, "myfuncs.ahk", "MySend() {\n  Send(\"{text}hi\")\n}")

	SetCustomScriptsDir(dir)
	inc, boot, fn := customBlocksOnce()

	if inc != "\n#Include ../data/scripts/myfuncs.ahk" {
		t.Fatalf("Include 块不符:\n%q", inc)
	}
	if strings.Contains(boot, "PluginManager.Register(") || strings.Contains(boot, "LoadEntry(") {
		t.Fatalf("函数库不得注册插件:\n%q", boot)
	}
	if !strings.Contains(boot, "未导出 Register(api), 仅作为函数库挂载") {
		t.Fatalf("缺函数库注记:\n%q", boot)
	}
	if !strings.Contains(fn, `"myfuncs", A_ScriptDir "\..\data\scripts\myfuncs.ahk"`) {
		t.Fatalf("名录分发缺函数库脚本:\n%q", fn)
	}
}

// 入口函数名单一真源: custom_functions.ahk 优先占名 (模板无条件 Include, 不可跳过),
// scripts/ 内按文件名字典序先到者胜; 后到者整体跳过 (否则 AHK 加载期重复定义拖垮引擎)。
func TestCustomScriptBlocks_DuplicateEntry(t *testing.T) {
	// 变体 a: 两个 scripts 脚本都导出 Register -> 字典序第一者胜
	dir := t.TempDir()
	writeCustomScript(t, dir, "a_first.ahk", "Register(api) {}")
	writeCustomScript(t, dir, "b_second.ahk", "Register(api) {}")
	SetCustomScriptsDir(dir)
	inc, boot, _ := customBlocksOnce()
	if !strings.Contains(inc, "a_first.ahk") || strings.Contains(inc, "b_second.ahk") {
		t.Fatalf("Include 裁决错误:\n%q", inc)
	}
	if !strings.Contains(boot, "custom:a_first") || !strings.Contains(boot, "b_second.ahk: 入口 Register(api) 与其他载荷重名") {
		t.Fatalf("引导块裁决错误:\n%q", boot)
	}

	// 变体 b: custom_functions.ahk 已导出 Register -> scripts 脚本全部让位
	dir = t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "custom_functions.ahk"), []byte("Register(api) {}"), 0o644); err != nil {
		t.Fatalf("写约定文件失败: %v", err)
	}
	writeCustomScript(t, dir, "s.ahk", "Register(api) {}")
	SetCustomScriptsDir(dir)
	inc, boot, _ = customBlocksOnce()
	if strings.Contains(inc, "scripts/s.ahk") {
		t.Fatalf("与约定文件重名时不得 Include:\n%q", inc)
	}
	if !strings.Contains(boot, "s.ahk: 入口 Register(api) 与其他载荷重名") {
		t.Fatalf("缺重名警告:\n%q", boot)
	}
}

// 停用持久化 (config.options.plugins.disabled): 合成 ID 带 custom: 前缀, 停用脚本不注入
// 不注册不进名录 (落注释), 启用脚本不受影响 —— 与插件启停同一份数据链路 (约束 5)。
func TestCustomScriptBlocks_Disabled(t *testing.T) {
	dir := t.TempDir()
	writeCustomScript(t, dir, "on.ahk", "Register(api) {}")
	writeCustomScript(t, dir, "off.ahk", "Register(api) {}")

	Cfg = &model.Config{Options: model.Options{Plugins: model.PluginsOption{Disabled: []string{"custom:off"}}}}
	defer func() { Cfg = nil }()
	SetCustomScriptsDir(dir)
	inc, boot, fn := customBlocksOnce()

	if strings.Contains(inc, "scripts/off.ahk") {
		t.Fatalf("停用脚本不得产出 Include:\n%q", inc)
	}
	if !strings.Contains(inc, "scripts/on.ahk") {
		t.Fatalf("启用脚本应正常注入:\n%q", inc)
	}
	if !strings.Contains(boot, "[外接脚本] custom:off 已在配置中停用") {
		t.Fatalf("缺停用注释:\n%q", boot)
	}
	if strings.Contains(boot, "Register(Map(\"id\", \"custom:off\"") {
		t.Fatalf("停用脚本不得注册:\n%q", boot)
	}
	if strings.Contains(fn, `"off", A_ScriptDir`) {
		t.Fatalf("停用脚本不得进名录:\n%q", fn)
	}
	if !strings.Contains(fn, `"on", A_ScriptDir`) {
		t.Fatalf("启用脚本应进名录:\n%q", fn)
	}
}

// 约定文件自愈: 模板对 custom_functions.ahk 的 #Include 无条件, 缺失 = 引擎加载失败,
// 生成端补仅注释空桩; 已存在的文件绝不触碰。
func TestCustomScriptBlocks_StubSelfHeal(t *testing.T) {
	// 缺失 -> 自愈 (UTF-8 BOM + 仅注释, 无代码载荷)
	dir := t.TempDir()
	writeCustomScript(t, dir, "x.ahk", "X() {}")
	SetCustomScriptsDir(dir)
	customBlocksOnce()
	raw, err := os.ReadFile(filepath.Join(dir, "custom_functions.ahk"))
	if err != nil {
		t.Fatalf("自愈桩未创建: %v", err)
	}
	if !strings.HasPrefix(string(raw), "\xef\xbb\xbf; 自定义的函数写在这个文件里") {
		t.Fatalf("自愈桩内容不符: %q", raw)
	}
	if strings.Contains(string(raw), "() {") || strings.Contains(string(raw), "Send(") {
		t.Fatalf("自愈桩不得含代码 (约束 7): %q", raw)
	}

	// 已存在 -> 绝不覆盖
	dir = t.TempDir()
	marker := "\xef\xbb\xbfMyFunc() {}"
	if err := os.WriteFile(filepath.Join(dir, "custom_functions.ahk"), []byte(marker), 0o644); err != nil {
		t.Fatalf("写约定文件失败: %v", err)
	}
	SetCustomScriptsDir(dir)
	customBlocksOnce()
	raw, err = os.ReadFile(filepath.Join(dir, "custom_functions.ahk"))
	if err != nil || string(raw) != marker {
		t.Fatalf("既有约定文件被改动: %q err=%v", raw, err)
	}
}

// 文件名校验: 仅平铺 *.ahk; 空基名 / 路径分隔符 / 盘符 / NUL 拒绝 (纵深防御,
// 与 plugins.safePluginRelFile 同口径)。
func TestValidCustomScriptName(t *testing.T) {
	ok := []string{"foo.ahk", "FOO.AHK", "my task 备份.ahk", "v1.2.ahk"}
	bad := []string{".ahk", "..ahk", "notes.txt", "a/b.ahk", `a\b.ahk`, "a:b.ahk", "a\x00.ahk"}
	for _, n := range ok {
		if !validCustomScriptName(n) {
			t.Errorf("%q 应为合法脚本名", n)
		}
	}
	for _, n := range bad {
		if validCustomScriptName(n) {
			t.Errorf("%q 应被拒绝", n)
		}
	}
}

// 非 .ahk 与子目录静默跳过 (不产噪声注释); 扫描顺序 = 文件名字典序 (os.ReadDir 保证), 块确定。
func TestCustomScriptBlocks_SkipNonScripts(t *testing.T) {
	dir := t.TempDir()
	writeCustomScript(t, dir, "keep.ahk", "K() {}")
	sdir := filepath.Join(dir, "scripts")
	os.WriteFile(filepath.Join(sdir, "README.md"), []byte("docs"), 0o644)
	os.MkdirAll(filepath.Join(sdir, "sub"), 0o755)
	os.WriteFile(filepath.Join(sdir, "sub", "nested.ahk"), []byte("N() {}"), 0o644)

	SetCustomScriptsDir(dir)
	inc, boot, fn := customBlocksOnce()

	if inc != "\n#Include ../data/scripts/keep.ahk" {
		t.Fatalf("Include 块不符:\n%q", inc)
	}
	if strings.Contains(boot, "README") || strings.Contains(boot, "nested") || strings.Contains(fn, "nested") {
		t.Fatalf("非平铺 .ahk 应静默跳过:\nboot=%q fn=%q", boot, fn)
	}
}
