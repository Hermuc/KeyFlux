package behaviors

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// writePack 写一个测试包目录 (dirName=id)
func writePack(t *testing.T, dir, id, manifest string) {
	t.Helper()
	d := filepath.Join(dir, id)
	if err := os.MkdirAll(d, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(d, "behavior.json"), []byte(manifest), 0o644); err != nil {
		t.Fatal(err)
	}
}

const psEditManifest = `{
  "id": "ps_edit", "name": "PS 编辑图片", "specVersion": 1,
  "appliesTo": [{"type": "fileExt", "exts": ["jpg", "png"]}],
  "entry": {"kind": "builtin", "action": "run", "params": {"actionValue": "Photoshop.exe \"%selected%\""}}
}`

const pngOnlyManifest = `{
  "id": "png_viewer", "name": "PNG 查看器", "specVersion": 1,
  "appliesTo": [{"type": "fileExt", "exts": ["png"]}],
  "entry": {"kind": "builtin", "action": "open"}
}`

func TestLoadCatalogOrderAndErrors(t *testing.T) {
	builtin := t.TempDir()
	user := t.TempDir()
	writePack(t, builtin, "open_url", `{"id":"open_url","name":"打开网址","specVersion":1,"appliesTo":[{"type":"textType","value":"url","default":true}],"entry":{"kind":"builtin","action":"open_url"}}`)
	writePack(t, user, "ps_edit", psEditManifest)
	writePack(t, user, "zzz_last", `{"id":"zzz_last","name":"Z","specVersion":1,"appliesTo":[{"type":"textType","value":"plain"}],"entry":{"kind":"builtin","action":"copy"}}`)
	// 坏包: 目录名与 id 不一致 → 跳过并记录
	writePack(t, user, "bad_dir", `{"id":"other","name":"X","specVersion":1,"appliesTo":[{"type":"textType","value":"plain"}],"entry":{"kind":"builtin","action":"copy"}}`)

	c := LoadCatalog(builtin, user)
	if len(c.Errors) != 1 || !strings.Contains(c.Errors[0], "bad_dir") {
		t.Fatalf("期望 1 条坏包错误, got %v", c.Errors)
	}
	if got := [3]string{c.Packs[0].ID, c.Packs[1].ID, c.Packs[2].ID}; got != [3]string{"open_url", "ps_edit", "zzz_last"} {
		t.Fatalf("内置在前+各自字典序, got %v", got)
	}
	if c.Packs[0].Source != "builtin" || c.Packs[1].Source != "user" {
		t.Fatalf("source 标记错误")
	}
	// 缺失目录 = 空目录 (不报错)
	if c2 := LoadCatalog(filepath.Join(builtin, "nope"), filepath.Join(user, "nope")); len(c2.Packs) != 0 || len(c2.Errors) != 0 {
		t.Fatalf("缺失目录应得空目录, got %v / %v", c2.Packs, c2.Errors)
	}
}

// 插件贡献包 (extraDirs, 声明式 contributes): 排在 builtin/user 之后, Source 标记 plugin*;
// 与用户包同 ID 冲突时先到者胜 (Get 取首个, 插件贡献不可遮蔽用户包);
// 插件贡献包非 user 来源 → 不可经行为库删除 (随插件装删);
// 未注册进目录的 plugin: 前缀动作类型经 ResolveRuleAction 原样透传 (IAction 运行时接线的前提)。
func TestLoadCatalog_PluginExtraDirs(t *testing.T) {
	builtin := t.TempDir()
	user := t.TempDir()
	plugA := t.TempDir()
	plugB := t.TempDir()
	writePack(t, builtin, "open_url", `{"id":"open_url","name":"打开网址","specVersion":1,"appliesTo":[{"type":"textType","value":"url"}],"entry":{"kind":"builtin","action":"open_url"}}`)
	writePack(t, user, "ps_edit", psEditManifest)
	// 插件 A 贡献两个包; 插件 B 贡献一个与用户包同 ID 的遮蔽测试包
	writePack(t, plugA, "everything_search", `{"id":"everything_search","name":"Everything 搜索","specVersion":1,"appliesTo":[{"type":"textType","value":"plain"}],"entry":{"kind":"builtin","action":"run","params":{"actionValue":"everything.exe -s \"%selected%\""}}}`)
	writePack(t, plugA, "open_with_notepad", `{"id":"open_with_notepad","name":"记事本打开","specVersion":1,"appliesTo":[{"type":"fileExt","exts":["txt"]},{"type":"textType","value":"plain"}],"entry":{"kind":"builtin","action":"open"}}`)
	writePack(t, plugB, "ps_edit", `{"id":"ps_edit","name":"冒名 PS","specVersion":1,"appliesTo":[{"type":"textType","value":"plain"}],"entry":{"kind":"builtin","action":"copy"}}`)

	c := LoadCatalog(builtin, user, plugA, plugB)
	if len(c.Errors) != 1 || !strings.Contains(c.Errors[0], "贡献包 ID \"ps_edit\" 与既有行为冲突") {
		t.Fatalf("期望 1 条贡献包冲突注记, got %v", c.Errors)
	}
	var ids []string
	for _, p := range c.Packs {
		ids = append(ids, p.ID)
	}
	want := []string{"open_url", "ps_edit", "everything_search", "open_with_notepad"}
	if strings.Join(ids, ",") != strings.Join(want, ",") {
		t.Fatalf("顺序不符 (builtin→user→各插件字典序, 冲突包去重): got %v", ids)
	}
	// Source 标记: 插件 A 的包 = plugin1
	if got := c.Get("everything_search").Source; got != "plugin1" {
		t.Fatalf("插件贡献包 Source = %q, want plugin1", got)
	}
	// 同 ID 遮蔽: ps_edit 解析到用户包 (先到者胜)
	if got := c.Get("ps_edit"); got.Source != "user" || got.Name != "PS 编辑图片" {
		t.Fatalf("ps_edit 应解析到用户包, got Source=%s Name=%s", got.Source, got.Name)
	}
	// 插件贡献包不可经行为库删除 (非 user 来源)
	if err := ValidateDelete(c, "everything_search", nil); err == nil || !strings.Contains(err.Error(), "不可删除") {
		t.Fatalf("插件贡献包应不可删除, got %v", err)
	}
	// 未入目录的 plugin: 前缀动作类型原样透传 (IAction 运行时接线前提)
	a, av, wd := ResolveRuleAction(c, "plugin:sample_greeter:timestamp", "tpl", "wd")
	if a != "plugin:sample_greeter:timestamp" || av != "tpl" || wd != "wd" {
		t.Fatalf("plugin: 前缀应透传, got %q/%q/%q", a, av, wd)
	}
}

func TestCovers(t *testing.T) {
	c := LoadCatalog(t.TempDir(), t.TempDir())
	c.Packs = append(c.Packs, &Pack{ID: "ps_edit", AppliesTo: []AppliesToEntry{{Type: "fileExt", Exts: []string{"jpg", "png"}}}, Entry: Entry{Kind: "builtin", Action: "run"}})

	cases := []struct {
		id, matchType string
		values        []string
		want          bool
	}{
		{"ps_edit", "fileExt", []string{"jpg"}, true},         // 规则值集 ⊆ 前提
		{"ps_edit", "fileExt", []string{"JPG"}, true},         // 大小写不敏感
		{"ps_edit", "fileExt", []string{"jpg", "png"}, true},  // 多值全覆盖
		{"ps_edit", "fileExt", []string{"jpg", "gif"}, false}, // 部分覆盖 = 不覆盖
		{"ps_edit", "fileExt", []string{"*"}, false},          // 任意文件规则不能依赖限定后缀的行为
		{"ps_edit", "textType", []string{"url"}, false},       // 类型不匹配
	}
	for _, tc := range cases {
		if got := c.Covers(tc.id, tc.matchType, tc.values); got != tc.want {
			t.Errorf("Covers(%s,%s,%v)=%v, want %v", tc.id, tc.matchType, tc.values, got, tc.want)
		}
	}
	// 通配前提覆盖任意后缀
	c.Packs = append(c.Packs, &Pack{ID: "any_file", AppliesTo: []AppliesToEntry{{Type: "fileExt", Exts: []string{"*"}}}})
	if !c.Covers("any_file", "fileExt", []string{"jpg", "gif"}) {
		t.Error("通配前提应覆盖任意后缀")
	}
	// 未知 ID
	if c.Covers("nope", "fileExt", []string{"jpg"}) {
		t.Error("未知行为不应覆盖任何前提")
	}
}

func TestDefaultFor(t *testing.T) {
	c := LoadCatalog(t.TempDir(), t.TempDir())
	c.Packs = []*Pack{
		{ID: "open_url", AppliesTo: []AppliesToEntry{{Type: "textType", Value: "url", Default: true}}},
		{ID: "search", AppliesTo: []AppliesToEntry{{Type: "textType", Value: "url"}, {Type: "textType", Value: "plain", Default: true}}},
	}
	if got := c.DefaultFor("textType", []string{"url"}); got != "open_url" {
		t.Fatalf("url 默认应为 open_url, got %s", got)
	}
	if got := c.DefaultFor("textType", []string{"plain"}); got != "search" {
		t.Fatalf("plain 默认应为 search, got %s", got)
	}
	// 无 default 标记 → 回退第一条覆盖包
	c2 := &Catalog{Packs: []*Pack{{ID: "b", AppliesTo: []AppliesToEntry{{Type: "textType", Value: "plain"}}}, {ID: "a", AppliesTo: []AppliesToEntry{{Type: "textType", Value: "plain"}}}}}
	if got := c2.DefaultFor("textType", []string{"plain"}); got != "b" {
		t.Fatalf("无标记应回退目录序第一条覆盖包, got %s", got)
	}
}

func TestResolveRuleAction(t *testing.T) {
	c := &Catalog{Packs: []*Pack{
		{ID: "ps_edit", Entry: Entry{Kind: "builtin", Action: "run", Params: EntryParams{ActionValue: "PS.exe %selected%", WorkingDir: "C:\\tools"}}},
	}}
	// 内置 ID 直通 (golden/plan 稳定性关键)
	if a, v, w := ResolveRuleAction(c, "run", "x.exe", ""); a != "run" || v != "x.exe" || w != "" {
		t.Fatal("内置 ID 必须直通")
	}
	// nil 目录直通 (golden 测试路径)
	if a, _, _ := ResolveRuleAction(nil, "custom_x", "", ""); a != "custom_x" {
		t.Fatal("nil 目录应原样返回")
	}
	// 用户包: 空值用包默认, 非空覆盖
	if a, v, w := ResolveRuleAction(c, "ps_edit", "", ""); a != "run" || v != "PS.exe %selected%" || w != `C:\tools` {
		t.Fatalf("包默认模板未生效: %s/%s/%s", a, v, w)
	}
	if a, v, _ := ResolveRuleAction(c, "ps_edit", "custom.exe", ""); a != "run" || v != "custom.exe" {
		t.Fatalf("规则覆盖应优先: %s/%s", a, v)
	}
	// 未知 ID / script entry 原样返回
	if a, _, _ := ResolveRuleAction(c, "ghost", "", ""); a != "ghost" {
		t.Fatal("未知 ID 应原样返回")
	}
	c.Packs = append(c.Packs, &Pack{ID: "scr", Entry: Entry{Kind: "script", File: "main.ahk", Func: "BehaviorMain"}})
	if a, _, _ := ResolveRuleAction(c, "scr", "", ""); a != "scr" {
		t.Fatal("script entry 一期应原样返回")
	}
}

func TestValidateManifest(t *testing.T) {
	base := func(mutate func(*Pack)) *Pack {
		p := &Pack{ID: "ok_id", Name: "N", SpecVersion: 1,
			AppliesTo: []AppliesToEntry{{Type: "fileExt", Exts: []string{"jpg"}}},
			Entry:     Entry{Kind: "builtin", Action: "run"}}
		mutate(p)
		return p
	}
	if err := ValidateManifest(base(func(*Pack) {})); err != nil {
		t.Fatalf("合法 manifest 不应报错: %v", err)
	}
	cases := []struct {
		name   string
		mutate func(*Pack)
	}{
		{"坏 ID", func(p *Pack) { p.ID = "Bad-Id" }},
		{"specVersion", func(p *Pack) { p.SpecVersion = 2 }},
		{"缺名称", func(p *Pack) { p.Name = " " }},
		{"缺前提", func(p *Pack) { p.AppliesTo = nil }},
		{"空后缀", func(p *Pack) { p.AppliesTo[0].Exts = []string{"  "} }},
		{"未知特征", func(p *Pack) { p.AppliesTo[0] = AppliesToEntry{Type: "textType", Value: "hash"} }},
		{"未知前提类型", func(p *Pack) { p.AppliesTo[0].Type = "regex" }},
		{"未知基础动作", func(p *Pack) { p.Entry.Action = "teleport" }},
		{"未知 entry kind", func(p *Pack) { p.Entry.Kind = "wasm" }},
		{"script 缺 func", func(p *Pack) { p.Entry = Entry{Kind: "script", File: "main.ahk"} }},
	}
	for _, tc := range cases {
		if err := ValidateManifest(base(tc.mutate)); err == nil {
			t.Errorf("%s: 应拒绝", tc.name)
		}
	}
}

func TestValidateDelete(t *testing.T) {
	builtin := t.TempDir()
	user := t.TempDir()
	writePack(t, builtin, "open_url", `{"id":"open_url","name":"打开网址","specVersion":1,"appliesTo":[{"type":"textType","value":"url","default":true}],"entry":{"kind":"builtin","action":"open_url"}}`)
	writePack(t, user, "ps_edit", psEditManifest)     // 前提 jpg+png
	writePack(t, user, "png_viewer", pngOnlyManifest) // 前提 png
	c := LoadCatalog(builtin, user)

	// refs 手构造 RuleRef: 生产环境中这些引用由 server.behaviorRuleRefs 从
	// config.selectedAction.mappings.entries 投影而来 (MatchType/MatchValue 取自 mapping,
	// ActionType 取自 entry.Behavior); 本测试直接给等价引用以隔离行为目录逻辑。
	refs := func(rr ...RuleRef) []RuleRef { return rr }

	// 1. 内置不可删
	if err := ValidateDelete(c, "open_url", nil); err == nil || !strings.Contains(err.Error(), "内置") {
		t.Fatalf("内置包删除应被拒绝: %v", err)
	}
	// 2. 被规则引用 → 拒绝
	if err := ValidateDelete(c, "ps_edit", refs(RuleRef{"fileExt", "jpg", "ps_edit"})); err == nil || !strings.Contains(err.Error(), "引用") {
		t.Fatalf("被引用行为删除应被拒绝: %v", err)
	}
	// 3. 值级覆盖: 删 ps_edit (jpg+png) 时 png 仍被 png_viewer 覆盖, 但 jpg 无覆盖 → 拒绝并点名 jpg
	err := ValidateDelete(c, "ps_edit", refs(RuleRef{"fileExt", "jpg,png", "open"}))
	if err == nil || !strings.Contains(err.Error(), ".jpg") {
		t.Fatalf("应报 jpg 空前提桶: %v", err)
	}
	// 4. png_viewer 无引用且其前提无人引用 → 允许删除
	if err := ValidateDelete(c, "png_viewer", refs(RuleRef{"fileExt", "jpg,png", "ps_edit"})); err != nil {
		t.Fatalf("无引用前提应允许删除: %v", err)
	}
	// 5. 仅有规则引用 png (jpg 无人引用): 删 ps_edit 时 png 断档被点名, jpg 随包消失不报
	c2 := &Catalog{Packs: []*Pack{c.Get("open_url"), c.Get("ps_edit")}, Errors: c.Errors}
	err = ValidateDelete(c2, "ps_edit", refs(RuleRef{"fileExt", "png", "open"}))
	if err == nil || !strings.Contains(err.Error(), ".png") || strings.Contains(err.Error(), ".jpg") {
		t.Fatalf("应只报 png 断档 (jpg 无人引用随包消失): %v", err)
	}
}

func TestWriteAndRemoveUserPack(t *testing.T) {
	user := t.TempDir()
	p := &Pack{ID: "ps_edit", Name: "PS", SpecVersion: 1,
		AppliesTo: []AppliesToEntry{{Type: "fileExt", Exts: []string{".jpg"}}}, // 带点应原样保存 (覆盖比较时归一化)
		Entry:     Entry{Kind: "builtin", Action: "run"}}
	if err := WriteUserPack(user, p); err != nil {
		t.Fatalf("写入失败: %v", err)
	}
	if _, err := os.Stat(filepath.Join(user, "ps_edit", "behavior.json")); err != nil {
		t.Fatal("manifest 未落盘")
	}
	// 与内置保留 ID 冲突
	p.ID = "run"
	if err := WriteUserPack(user, p); err == nil {
		t.Fatal("保留 ID 应拒绝")
	}
	// 删除
	if err := RemoveUserPack(user, "ps_edit"); err != nil {
		t.Fatalf("删除失败: %v", err)
	}
	if _, err := os.Stat(filepath.Join(user, "ps_edit", "behavior.json")); !os.IsNotExist(err) {
		t.Fatal("目录未删除")
	}
}
