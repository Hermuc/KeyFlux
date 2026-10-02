package plugins

import (
	"archive/zip"
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func validManifest() *Manifest {
	return &Manifest{
		ID: "hello_world", Name: "Hello", SpecVersion: 1,
		Entry: Entry{Kind: "script", File: "main.ahk", Func: "PluginMain"},
	}
}

func TestValidateManifest(t *testing.T) {
	if err := ValidateManifest(validManifest()); err != nil {
		t.Fatalf("合法 manifest 被拒绝: %v", err)
	}
	cases := []struct {
		mutate func(*Manifest)
		want   string
	}{
		{func(m *Manifest) { m.ID = "Bad-ID" }, "不合法"},
		{func(m *Manifest) { m.Bundled = true }, "bundled"},
		{func(m *Manifest) { m.SpecVersion = 2 }, "specVersion"},
		{func(m *Manifest) { m.Name = " " }, "缺少名称"},
		{func(m *Manifest) { m.Entry = Entry{Kind: "builtin"} }, "entry.kind"},
		{func(m *Manifest) { m.Entry = Entry{Kind: "script", File: "x.ahk"} }, "缺少 file 或 func"},
	}
	for i, tc := range cases {
		m := validManifest()
		tc.mutate(m)
		err := ValidateManifest(m)
		if err == nil || !strings.Contains(err.Error(), tc.want) {
			t.Fatalf("case %d: 期望错误含 %q, 实际: %v", i, tc.want, err)
		}
	}
}

// TestValidateProvidesAndLateInit P7a 协议扩容: provides.actions[] + entry.late
// 的校验矩阵与 wire 形态 (omitempty 零漂移)。
func TestValidateProvidesAndLateInit(t *testing.T) {
	// entry.late: 合法值放行; 非法标识符拒绝。
	m := validManifest()
	m.Entry.Late = "InitHelloWorld"
	if err := ValidateManifest(m); err != nil {
		t.Fatalf("合法 entry.late 被拒绝: %v", err)
	}
	for _, bad := range []string{"1Init", "Init X", "Init-X", strings.Repeat("A", 65)} {
		m.Entry.Late = bad
		if err := validateManifestBody(m); err == nil || !strings.Contains(err.Error(), "entry.late") {
			t.Fatalf("entry.late %q: 期望拒绝, 实际: %v", bad, err)
		}
	}

	// provides: 合法声明放行 (含 labelEn / 多动作)。
	m = validManifest()
	m.Provides = &Provides{Actions: []ProvidedAction{
		{ID: "goto", Label: "跳转", LabelEn: "Go", Kind: "plugin"},
		{ID: "back", Label: "返回", Kind: "plugin"},
	}}
	if err := ValidateManifest(m); err != nil {
		t.Fatalf("合法 provides 被拒绝: %v", err)
	}

	cases := []struct {
		mutate func(*Provides)
		want   string
	}{
		{func(p *Provides) { p.Actions = nil }, "没有任何 action"},
		{func(p *Provides) { p.Actions[0].ID = "Bad-Id" }, "不合法"},
		{func(p *Provides) { p.Actions[1].ID = p.Actions[0].ID }, "重复"},
		{func(p *Provides) { p.Actions[0].Label = " " }, "缺少 label"},
		{func(p *Provides) { p.Actions[0].Kind = "builtin" }, "kind"},
	}
	for i, tc := range cases {
		m = validManifest()
		m.Provides = &Provides{Actions: []ProvidedAction{
			{ID: "goto", Label: "跳转", Kind: "plugin"},
			{ID: "back", Label: "返回", Kind: "plugin"},
		}}
		tc.mutate(m.Provides)
		err := ValidateManifest(m)
		if err == nil || !strings.Contains(err.Error(), tc.want) {
			t.Fatalf("case %d: 期望错误含 %q, 实际: %v", i, tc.want, err)
		}
	}

	// wire 形态: 无 provides / 无 late 的 manifest 序列化不含新键 (零漂移)。
	raw := manifestJSON(t, validManifest())
	if strings.Contains(raw, "provides") || strings.Contains(raw, "late") {
		t.Fatalf("存量 manifest wire 漂移: %s", raw)
	}
	// 有声明时键出场, 顺序 = 声明序 (provides 在 settings 之后)。
	m = validManifest()
	m.Entry.Late = "InitHello"
	m.Provides = &Provides{Actions: []ProvidedAction{{ID: "goto", Label: "跳转", Kind: "plugin"}}}
	raw = manifestJSON(t, m)
	want := `"entry":{"kind":"script","file":"main.ahk","func":"PluginMain","late":"InitHello"}`
	if !strings.Contains(raw, want) {
		t.Fatalf("entry.late wire 形态不符: %s", raw)
	}
	want = `"provides":{"actions":[{"id":"goto","label":"跳转","kind":"plugin"}]}`
	if !strings.Contains(raw, want) || !strings.HasSuffix(raw, want+"}") {
		t.Fatalf("provides wire 形态/位置不符: %s", raw)
	}
}

// buildZip 构造内存 zip: entries 为 name -> content, 目录项用空 content。
func buildZip(t *testing.T, entries map[string]string) []byte {
	t.Helper()
	var buf bytes.Buffer
	zw := zip.NewWriter(&buf)
	for name, content := range entries {
		hdr := &zip.FileHeader{Name: name, Method: zip.Deflate}
		w, err := zw.CreateHeader(hdr)
		if err != nil {
			t.Fatal(err)
		}
		if content != "" {
			if _, err := w.Write([]byte(content)); err != nil {
				t.Fatal(err)
			}
		}
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
	return buf.Bytes()
}

func manifestJSON(t *testing.T, m *Manifest) string {
	t.Helper()
	raw, err := json.Marshal(m)
	if err != nil {
		t.Fatal(err)
	}
	return string(raw)
}

func TestInstallFromZip_FlatAndTopDir(t *testing.T) {
	user := t.TempDir()
	m := validManifest()

	// 扁平结构: plugin.json 在 zip 根
	flat := buildZip(t, map[string]string{
		"plugin.json": manifestJSON(t, m),
		"main.ahk":    "#Requires AutoHotkey v2.0\nPluginMain() {}\n",
	})
	got, err := InstallFromZip(bytes.NewReader(flat), user)
	if err != nil {
		t.Fatalf("扁平 zip 安装失败: %v", err)
	}
	if got.ID != m.ID {
		t.Fatalf("安装结果 ID 不一致: %s", got.ID)
	}
	if _, err := os.Stat(filepath.Join(user, m.ID, "main.ahk")); err != nil {
		t.Fatal("包内文件未落盘")
	}

	// 带唯一顶层目录 (GitHub 源码 zip 形态)
	m2 := validManifest()
	m2.ID = "top_dir_pack"
	top := buildZip(t, map[string]string{
		"top-dir/plugin.json":  manifestJSON(t, m2),
		"top-dir/lib/util.ahk": "#Requires AutoHotkey v2.0\n",
	})
	if _, err := InstallFromZip(bytes.NewReader(top), user); err != nil {
		t.Fatalf("顶层目录 zip 安装失败: %v", err)
	}
	if _, err := os.Stat(filepath.Join(user, "top_dir_pack", "lib", "util.ahk")); err != nil {
		t.Fatal("子目录文件未落盘")
	}

	// 重复安装同 ID 拒绝
	if _, err := InstallFromZip(bytes.NewReader(flat), user); err == nil || !strings.Contains(err.Error(), "已存在") {
		t.Fatalf("重复安装应拒绝: %v", err)
	}
}

func TestInstallFromZip_Rejects(t *testing.T) {
	user := t.TempDir()
	m := validManifest()

	// 无 plugin.json
	noManifest := buildZip(t, map[string]string{"main.ahk": "x"})
	if _, err := InstallFromZip(bytes.NewReader(noManifest), user); err == nil || !strings.Contains(err.Error(), "plugin.json") {
		t.Fatalf("缺 manifest 应拒绝: %v", err)
	}

	// 路径穿越 (zip-slip)
	slip := buildZip(t, map[string]string{
		"plugin.json": manifestJSON(t, m),
		"../evil.txt": "x",
	})
	if _, err := InstallFromZip(bytes.NewReader(slip), user); err == nil || !strings.Contains(err.Error(), "不安全路径") {
		t.Fatalf("zip-slip 应拒绝: %v", err)
	}
	// 确认没有逃逸出 user 目录
	if _, err := os.Stat(filepath.Join(user, "..", "evil.txt")); !os.IsNotExist(err) {
		t.Fatal("zip-slip 文件逃逸到了插件目录外")
	}

	// 坏 manifest (specVersion 不符)
	m2 := validManifest()
	m2.SpecVersion = 9
	bad := buildZip(t, map[string]string{"plugin.json": manifestJSON(t, m2)})
	if _, err := InstallFromZip(bytes.NewReader(bad), user); err == nil || !strings.Contains(err.Error(), "specVersion") {
		t.Fatalf("坏 manifest 应拒绝: %v", err)
	}

	// 非 zip 内容
	if _, err := InstallFromZip(strings.NewReader("this is not a zip"), user); err == nil {
		t.Fatal("非 zip 内容应拒绝")
	}

	// 安装失败后不留临时目录 / 不落目标目录
	entries, err := os.ReadDir(user)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 0 {
		t.Fatalf("失败的安装不应留下任何目录, 实际 %d 个", len(entries))
	}
}

func TestLoadCatalogAndRemove(t *testing.T) {
	user := t.TempDir()

	// 目录缺失 = 空目录, 不报错
	c := LoadCatalog(user)
	if len(c.Plugins) != 0 || len(c.Errors) != 0 {
		t.Fatalf("空目录应无插件无错误: %+v", c)
	}

	// 正常安装 + 坏包共存: 错误隔离
	m := validManifest()
	pkg := buildZip(t, map[string]string{"plugin.json": manifestJSON(t, m), "main.ahk": "x"})
	if _, err := InstallFromZip(bytes.NewReader(pkg), user); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(user, "broken_pack"), 0o755); err != nil {
		t.Fatal(err)
	}
	c = LoadCatalog(user)
	if len(c.Plugins) != 1 || c.Plugins[0].ID != m.ID {
		t.Fatalf("应加载 1 个插件: %+v", c.Plugins)
	}
	if len(c.Errors) != 1 || !strings.Contains(c.Errors[0], "broken_pack") {
		t.Fatalf("坏包应记入错误: %+v", c.Errors)
	}

	// 删除: 不存在的 ID / 非法 ID / 合法删除
	if err := Remove(user, "quick_switch"); err == nil {
		t.Fatal("不存在的插件应拒绝删除")
	}
	if err := Remove(user, "../evil"); err == nil {
		t.Fatal("非法 ID 应拒绝删除")
	}
	if err := Remove(user, "ghost"); err == nil {
		t.Fatal("不存在的插件应拒绝删除")
	}
	if err := Remove(user, m.ID); err != nil {
		t.Fatalf("删除失败: %v", err)
	}
	if _, err := os.Stat(filepath.Join(user, m.ID)); !os.IsNotExist(err) {
		t.Fatal("目录未删除")
	}
}

// 目录加载放行随包插件 (以标准插件形态分发), 导入 API 拒绝 bundled 标记声明
// (2026-10-02 P7b: 「内置」不再用硬编码 ID 名单, 改分发标记动态判定)。
func TestBundledMarker_CatalogAllowsImportRejects(t *testing.T) {
	raw := []byte(`{
		"id": "quick_switch", "name": "快速切换", "specVersion": 1, "bundled": true,
		"entry": {"kind": "script", "file": "main.ahk", "func": "QuickSwitchMain"}
	}`)
	m, err := parseManifest(raw)
	if err != nil {
		t.Fatalf("目录加载应放行随包插件: %v", err)
	}
	if err := ValidateManifest(m); err == nil {
		t.Fatalf("导入 API 应拒绝 bundled 标记声明")
	}
	plain := *m
	plain.Bundled = false
	if err := ValidateManifest(&plain); err != nil {
		t.Fatalf("无标记的同结构 manifest 应通过: %v", err)
	}
}

// 2026-10-02 P4: Remove 放行内置 ID (随包内置插件可删除; 墓碑由面板 config
// 状态源负责, 本函数只删目录)。
func TestRemove_AllowsBuiltinID(t *testing.T) {
	dir := t.TempDir()
	pdir := filepath.Join(dir, "quick_switch")
	os.MkdirAll(pdir, 0o755)
	if err := Remove(dir, "quick_switch"); err != nil {
		t.Fatalf("内置 ID 删除应放行: %v", err)
	}
	if _, err := os.Stat(pdir); !os.IsNotExist(err) {
		t.Fatalf("目录应已删除")
	}
}
