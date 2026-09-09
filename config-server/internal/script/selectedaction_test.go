package script

import (
	"strings"
	"testing"

	"settings/internal/behaviors"
)

// ============ MigrateSelectedAction 迁移正确性 ============

func TestMigrateSingleSchemeSingleRule(t *testing.T) {
	cfg := &Config{
		ActionSchemes: []ActionScheme{{
			ID: 1, Name: "搜索选中", Hotkey: ">^p", Enable: true,
			Rules: []ActionRule{
				{Priority: 1, MatchType: "textType", MatchValue: "url", ActionType: "open_url"},
			},
		}},
	}
	MigrateSelectedAction(cfg)
	sa := cfg.SelectedAction
	if sa == nil {
		t.Fatal("迁移后 selectedAction 不应为 nil")
	}
	if sa.Hotkey != ">^p" || !sa.Enable {
		t.Fatalf("hotkey/enable 未取首个方案: %q/%v", sa.Hotkey, sa.Enable)
	}
	if len(sa.Mappings) != 1 || sa.Mappings[0].MatchType != "textType" || sa.Mappings[0].MatchValue != "url" {
		t.Fatalf("mapping 投影错误: %+v", sa.Mappings)
	}
	if len(sa.Mappings[0].Entries) != 1 || sa.Mappings[0].Entries[0].Behavior != "open_url" {
		t.Fatalf("entry 投影错误: %+v", sa.Mappings[0].Entries)
	}
	if cfg.ActionSchemes != nil {
		t.Fatal("迁移后 ActionSchemes 应置 nil (save 不再输出旧段)")
	}
}

func TestMigrateMergeDuplicatedMappings(t *testing.T) {
	cfg := &Config{
		ActionSchemes: []ActionScheme{{
			ID: 1, Hotkey: "^p", Enable: true,
			Rules: []ActionRule{
				{Priority: 1, MatchType: "fileExt", MatchValue: "jpg,png", ActionType: "open", Options: RuleOptions{CopyToClipboard: true}},
				{Priority: 2, MatchType: "fileExt", MatchValue: "jpg,png", ActionType: "run", ActionValue: "x.exe"},
				{Priority: 3, MatchType: "textType", MatchValue: "url", ActionType: "open_url"},
			},
		}},
	}
	MigrateSelectedAction(cfg)
	if len(cfg.SelectedAction.Mappings) != 2 {
		t.Fatalf("同 matchType+matchValue 应合并去重, 期望 2 个 mapping: %+v", cfg.SelectedAction.Mappings)
	}
	m0 := cfg.SelectedAction.Mappings[0]
	if m0.MatchType != "fileExt" || len(m0.Entries) != 2 {
		t.Fatalf("合并后 entries 应顺次追加: %+v", m0)
	}
	if m0.Entries[0].Behavior != "open" || m0.Entries[1].Behavior != "run" || m0.Entries[1].ActionValue != "x.exe" {
		t.Fatalf("entry 顺序/字段错误: %+v", m0.Entries)
	}
	// rule 的 options 应随 entry 保留
	if !m0.Entries[0].Options.CopyToClipboard {
		t.Fatal("rule.options 应迁移到 entry.options")
	}
}

func TestMigrateMultipleSchemesTakeFirstHotkey(t *testing.T) {
	cfg := &Config{
		ActionSchemes: []ActionScheme{
			{ID: 1, Hotkey: "^p", Enable: false},
			{ID: 2, Hotkey: "#f", Enable: true, Rules: []ActionRule{
				{MatchType: "textType", MatchValue: "url", ActionType: "open_url"},
			}},
		},
	}
	MigrateSelectedAction(cfg)
	sa := cfg.SelectedAction
	if sa.Hotkey != "^p" || sa.Enable {
		t.Fatalf("多方案应取首个方案的 hotkey/enable: %q/%v", sa.Hotkey, sa.Enable)
	}
	// 但规则来自全部方案 (规格: 每条 rule → 对应类型 mapping)
	if len(sa.Mappings) != 1 {
		t.Fatalf("其余方案的规则仍应迁移: %+v", sa.Mappings)
	}
}

func TestMigrateEmptySchemeAndFactoryDefault(t *testing.T) {
	// 空方案 (actionSchemes 存在但无方案)
	empty := &Config{ActionSchemes: []ActionScheme{}}
	MigrateSelectedAction(empty)
	if empty.SelectedAction == nil {
		t.Fatal("空方案迁移后 selectedAction 不应为 nil")
	}
	if empty.SelectedAction.Hotkey != "" || empty.SelectedAction.Enable || len(empty.SelectedAction.Mappings) != 0 {
		t.Fatalf("空方案应得零值 selectedAction: %+v", empty.SelectedAction)
	}
	if empty.SelectedAction.Mappings == nil {
		t.Fatal("mappings 应初始化为空切片 (save 恒输出 [])")
	}
	// 出厂默认: 无 actionSchemes 且缺 selectedAction 键
	factory := &Config{}
	MigrateSelectedAction(factory)
	if factory.SelectedAction == nil || len(factory.SelectedAction.Mappings) != 0 {
		t.Fatal("出厂缺键应得空 selectedAction")
	}
	if factory.ActionSchemes != nil {
		t.Fatal("迁移后 ActionSchemes 应置 nil")
	}
}

func TestMigrateExistingSelectedActionWins(t *testing.T) {
	cfg := &Config{
		SelectedAction: &SelectedAction{Hotkey: ">^p", Enable: true, Mappings: []SelectedMapping{
			{MatchType: "textType", MatchValue: "url", Entries: []SelectedEntry{{Behavior: "open_url"}}},
		}},
		ActionSchemes: []ActionScheme{{ID: 1, Hotkey: "^old", Enable: true, Rules: []ActionRule{
			{MatchType: "textType", MatchValue: "url", ActionType: "search"},
		}}},
	}
	MigrateSelectedAction(cfg)
	if cfg.SelectedAction.Hotkey != ">^p" || len(cfg.SelectedAction.Mappings) != 1 || cfg.SelectedAction.Mappings[0].Entries[0].Behavior != "open_url" {
		t.Fatalf("已有 selectedAction 键应直接采用, 旧段忽略: %+v", cfg.SelectedAction)
	}
	if cfg.ActionSchemes != nil {
		t.Fatal("已有 selectedAction 时旧段也应置 nil (save 不再输出)")
	}
}

// ============ ValidateSelectedAction 三则校验 ============

func catalogWithPsEdit() *behaviors.Catalog {
	return &behaviors.Catalog{Packs: []*behaviors.Pack{{
		ID: "ps_edit", Name: "PS 编辑图片",
		AppliesTo: []behaviors.AppliesToEntry{{Type: "fileExt", Exts: []string{"jpg", "png"}}},
		Entry:     behaviors.Entry{Kind: "builtin", Action: "run"},
	}}}
}

func TestValidateCoverageFailure(t *testing.T) {
	sa := &SelectedAction{Hotkey: ">^p", Enable: true, Mappings: []SelectedMapping{
		{MatchType: "fileExt", MatchValue: "jpg,gif", Entries: []SelectedEntry{{Behavior: "ps_edit"}}},
	}}
	err := ValidateSelectedAction(sa, catalogWithPsEdit())
	if err == nil || !strings.Contains(err.Error(), "不匹配") {
		t.Fatalf("gif 不在 ps_edit 前提内应拒绝: %v", err)
	}
	// 未知行为 ID (非内置, 目录中不存在) 同样拒绝
	sa.Mappings[0].MatchValue = "jpg"
	sa.Mappings[0].Entries[0].Behavior = "ghost"
	if err := ValidateSelectedAction(sa, catalogWithPsEdit()); err == nil {
		t.Fatal("目录外行为应拒绝")
	}
}

func TestValidateEntriesRange(t *testing.T) {
	// 空 entries
	sa := &SelectedAction{Hotkey: ">^p", Enable: true, Mappings: []SelectedMapping{
		{MatchType: "textType", MatchValue: "url", Entries: []SelectedEntry{}},
	}}
	if err := ValidateSelectedAction(sa, nil); err == nil || !strings.Contains(err.Error(), "没有行为") {
		t.Fatalf("空 entries 应拒绝: %v", err)
	}
	// 超 9 项
	entries := make([]SelectedEntry, 10)
	for i := range entries {
		entries[i] = SelectedEntry{Behavior: "open_url"}
	}
	sa.Mappings[0].Entries = entries
	if err := ValidateSelectedAction(sa, nil); err == nil || !strings.Contains(err.Error(), "超过 9") {
		t.Fatalf("超 9 项应拒绝: %v", err)
	}
	// 恰好 9 项放行
	sa.Mappings[0].Entries = entries[:9]
	if err := ValidateSelectedAction(sa, nil); err != nil {
		t.Fatalf("9 项应放行: %v", err)
	}
}

func TestValidateHotkeyNonEmptyWhenEnabled(t *testing.T) {
	sa := &SelectedAction{Hotkey: "", Enable: true}
	if err := ValidateSelectedAction(sa, nil); err == nil || !strings.Contains(err.Error(), "热键") {
		t.Fatalf("启用+空热键应拒绝: %v", err)
	}
	// 禁用 + 空热键放行 (空方案/出厂默认态可保存, 不锁死存量)
	sa.Enable = false
	if err := ValidateSelectedAction(sa, nil); err != nil {
		t.Fatalf("禁用+空热键应放行: %v", err)
	}
}

func TestValidateNilCatalogTolerant(t *testing.T) {
	// cat=nil 跳过覆盖检查, 仅做词表校验 (镜像旧 actionscheme.go 的 cat==nil 容忍)
	sa := &SelectedAction{Hotkey: ">^p", Enable: true, Mappings: []SelectedMapping{
		{MatchType: "textType", MatchValue: "url", Entries: []SelectedEntry{{Behavior: "open_url"}}},
	}}
	if err := ValidateSelectedAction(sa, nil); err != nil {
		t.Fatalf("nil 目录应跳过覆盖检查: %v", err)
	}
	// textType 未知特征仍拒绝
	sa.Mappings[0].MatchValue = "hash"
	if err := ValidateSelectedAction(sa, nil); err == nil {
		t.Fatal("未知文本特征应拒绝")
	}
}

// ---- M1: 同 (matchType, matchValue) mapping 重复拒绝 ----

func TestValidateDuplicateMappingRejected(t *testing.T) {
	sa := &SelectedAction{Hotkey: ">^p", Enable: true, Mappings: []SelectedMapping{
		{MatchType: "textType", MatchValue: "url", Entries: []SelectedEntry{{Behavior: "open_url"}}},
		{MatchType: "textType", MatchValue: "url", Entries: []SelectedEntry{{Behavior: "search"}}},
	}}
	err := ValidateSelectedAction(sa, nil)
	if err == nil || !strings.Contains(err.Error(), "重复") || !strings.Contains(err.Error(), "url") {
		t.Fatalf("同 (matchType,matchValue) 重复应拒绝且错误含重复值: %v", err)
	}
	// 大小写/首尾空白归一后仍视为重复 (与运行时匹配的忽略大小写语义一致)
	sa.Mappings[1].MatchValue = " URL "
	if err := ValidateSelectedAction(sa, nil); err == nil || !strings.Contains(err.Error(), "重复") {
		t.Fatalf("大小写/空白归一后仍应视为重复: %v", err)
	}
	// fileExt 同值重复同样拒绝
	dupExt := &SelectedAction{Hotkey: ">^p", Enable: true, Mappings: []SelectedMapping{
		{MatchType: "fileExt", MatchValue: "jpg,png", Entries: []SelectedEntry{{Behavior: "open"}}},
		{MatchType: "fileExt", MatchValue: "JPG,PNG", Entries: []SelectedEntry{{Behavior: "run"}}},
	}}
	if err := ValidateSelectedAction(dupExt, nil); err == nil || !strings.Contains(err.Error(), "重复") {
		t.Fatalf("fileExt 大小写归一后重复应拒绝: %v", err)
	}
}

func TestValidateDistinctMappingsPass(t *testing.T) {
	// 合法多样: textType 两种特征 + fileExt 一组后缀
	sa := &SelectedAction{Hotkey: ">^p", Enable: true, Mappings: []SelectedMapping{
		{MatchType: "textType", MatchValue: "url", Entries: []SelectedEntry{{Behavior: "open_url"}}},
		{MatchType: "textType", MatchValue: "plain", Entries: []SelectedEntry{{Behavior: "run"}}},
		{MatchType: "fileExt", MatchValue: "jpg,png", Entries: []SelectedEntry{{Behavior: "open"}}},
	}}
	if err := ValidateSelectedAction(sa, nil); err != nil {
		t.Fatalf("合法多样 mapping 应通过: %v", err)
	}
	// 同类型不同值不算重复
	sameType := &SelectedAction{Hotkey: ">^p", Enable: true, Mappings: []SelectedMapping{
		{MatchType: "fileExt", MatchValue: "jpg", Entries: []SelectedEntry{{Behavior: "open"}}},
		{MatchType: "fileExt", MatchValue: "png", Entries: []SelectedEntry{{Behavior: "open"}}},
	}}
	if err := ValidateSelectedAction(sameType, nil); err != nil {
		t.Fatalf("同类型不同值不应判为重复: %v", err)
	}
}

// ---- L1: fileExt 条件值归一化后为空拒绝 ----

func TestValidateFileExtEmptyValueRejected(t *testing.T) {
	for _, bad := range []string{"", "   ", "...", ",,,", " . , . "} {
		sa := &SelectedAction{Hotkey: ">^p", Enable: true, Mappings: []SelectedMapping{
			{MatchType: "fileExt", MatchValue: bad, Entries: []SelectedEntry{{Behavior: "open"}}},
		}}
		err := ValidateSelectedAction(sa, nil)
		if err == nil || !strings.Contains(err.Error(), "文件后缀") {
			t.Fatalf("fileExt 条件值 %q 归一化后为空应拒绝: %v", bad, err)
		}
	}
	// 合法值放行 (含点/空白混排: ".jpg, .png" 归一化后非空)
	ok := &SelectedAction{Hotkey: ">^p", Enable: true, Mappings: []SelectedMapping{
		{MatchType: "fileExt", MatchValue: ".jpg, .png", Entries: []SelectedEntry{{Behavior: "open"}}},
	}}
	if err := ValidateSelectedAction(ok, nil); err != nil {
		t.Fatalf("含点的合法后缀列表应通过: %v", err)
	}
	// 通配符 "*" 放行
	wild := &SelectedAction{Hotkey: ">^p", Enable: true, Mappings: []SelectedMapping{
		{MatchType: "fileExt", MatchValue: "*", Entries: []SelectedEntry{{Behavior: "open"}}},
	}}
	if err := ValidateSelectedAction(wild, nil); err != nil {
		t.Fatalf("通配符 * 应通过: %v", err)
	}
}

// ============ MatchSelectedAction 匹配 ============

func TestMatchSelectedAction(t *testing.T) {
	sa := &SelectedAction{Mappings: []SelectedMapping{
		{MatchType: "textType", MatchValue: "url", Entries: []SelectedEntry{{Behavior: "open_url"}}},
		{MatchType: "fileExt", MatchValue: "jpg,png", Entries: []SelectedEntry{{Behavior: "open"}}},
	}}
	// 文本 URL 命中第一个 mapping
	if m := MatchSelectedAction(sa, false, "https://example.com"); m == nil || m.MatchType != "textType" {
		t.Fatalf("URL 文本应命中 textType mapping: %+v", m)
	}
	// 文件命中第二个 mapping
	if m := MatchSelectedAction(sa, true, `C:\pic\a.JPG`); m == nil || m.MatchType != "fileExt" {
		t.Fatalf("文件应命中 fileExt mapping: %+v", m)
	}
	// 文本不命中 fileExt, 文件不命中 textType
	if m := MatchSelectedAction(sa, true, "https://example.com"); m != nil {
		t.Fatal("文件不应命中 textType mapping")
	}
	// 无命中
	if m := MatchSelectedAction(sa, false, "just plain text"); m != nil {
		t.Fatal("纯文本不应命中 url mapping")
	}
	// nil 容忍
	if m := MatchSelectedAction(nil, false, "https://example.com"); m != nil {
		t.Fatal("nil 输入应返回 nil")
	}
}
