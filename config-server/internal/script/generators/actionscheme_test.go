package generators

import (
	"strings"
	"testing"

	"settings/internal/behaviors"
	"settings/internal/script/model"
)

func sampleSelectedAction() *model.SelectedAction {
	return &model.SelectedAction{
		Hotkey: ">^p", Enable: true,
		Mappings: []model.SelectedMapping{
			{MatchType: "textType", MatchValue: "url", Entries: []model.SelectedEntry{
				{Behavior: "open_url"},
				{Behavior: "search", ActionValue: "https://b.com?q=%selected%"},
			}},
			{MatchType: "fileExt", MatchValue: "jpg,png", Entries: []model.SelectedEntry{
				{Behavior: "open", ActionValue: "%selected%"},
			}},
		},
	}
}

func TestSelectedActionCodeSkipsInactive(t *testing.T) {
	for _, sa := range []*model.SelectedAction{nil, {Hotkey: ">^p", Enable: false}, {Hotkey: "", Enable: true}} {
		if got := selectedActionCode(sa); got != "" {
			t.Fatalf("nil/禁用/空热键应输出空串, got %q", got)
		}
	}
}

func TestSelectedActionCodeDataColumns(t *testing.T) {
	got := selectedActionCode(sampleSelectedAction())
	// 关键列断言: 序号 (key 1-9)、behavior ID、展开后 action/模板、显示名 (目录缺失回退 ID)
	needles := []string{
		"SelectedActionInit(\">^p\", SelectedActionData)",
		`{matchType: "textType", matchValue: "url", key: 1, behavior: "open_url", action: "open_url", actionValue: "", workingDir: "", name: "open_url"}`,
		`{matchType: "textType", matchValue: "url", key: 2, behavior: "search", action: "search", actionValue: "https://b.com?q=%selected%", workingDir: "", name: "search"}`,
		`{matchType: "fileExt", matchValue: "jpg,png", key: 1, behavior: "open", action: "open", actionValue: "%selected%", workingDir: "", name: "open"}`,
	}
	for _, n := range needles {
		if !strings.Contains(got, n) {
			t.Errorf("产物缺少关键列:\n  期望包含: %s\n  实际产物:\n%s", n, got)
		}
	}
	// 稳定性: 两次调用逐字节一致 (可 diff 契约)
	if got != selectedActionCode(sampleSelectedAction()) {
		t.Fatal("渲染结果不稳定")
	}
}

func TestSelectedActionCodeUserPackExpansion(t *testing.T) {
	prev := BehaviorCatalog
	t.Cleanup(func() { BehaviorCatalog = prev })
	BehaviorCatalog = &behaviors.Catalog{Packs: []*behaviors.Pack{{
		ID: "ps_edit", Name: "PS 编辑图片",
		Entry: behaviors.Entry{Kind: "builtin", Action: "run",
			Params: behaviors.EntryParams{ActionValue: "PS.exe %selected%", WorkingDir: `C:\tools`}},
	}}}
	sa := &model.SelectedAction{Hotkey: ">^p", Enable: true, Mappings: []model.SelectedMapping{
		{MatchType: "fileExt", MatchValue: "jpg", Entries: []model.SelectedEntry{
			{Behavior: "ps_edit"},                       // 空值补包默认
			{Behavior: "ps_edit", ActionValue: "x.exe"}, // 非空覆盖包默认
		}},
	}}
	got := selectedActionCode(sa)
	needles := []string{
		`behavior: "ps_edit", action: "run", actionValue: "PS.exe %selected%", workingDir: "C:\tools", name: "PS 编辑图片"`,
		`behavior: "ps_edit", action: "run", actionValue: "x.exe", workingDir: "C:\tools", name: "PS 编辑图片"`,
	}
	for _, n := range needles {
		if !strings.Contains(got, n) {
			t.Errorf("用户包展开列错误:\n  期望包含: %s\n  实际产物:\n%s", n, got)
		}
	}
}
