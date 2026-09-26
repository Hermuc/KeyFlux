package generators

import (
	"testing"

	"settings/internal/script/model"
)

func TestSortActions(t *testing.T) {
	a := model.Action{WindowGroupID: 0, Comment: "item1"}
	b := model.Action{WindowGroupID: 1, Comment: "item2"}
	c := model.Action{WindowGroupID: 0, Comment: "item3"}
	s := []model.Action{a, b, c}
	ss := sortActions(s)
	assertEqual(t, ss[0], b)
	assertEqual(t, ss[1], a)
	assertEqual(t, ss[2], c)
}

func TestWinTitleWarning(t *testing.T) {
	// 组合串 "标题 ahk_exe 名.exe" 含 ahk_ 条件, 不应告警
	assertEqual(t, winTitleWarning("记事本 ahk_exe notepad.exe"), "")
	// 裸 xxx.exe (不含任何 ahk_ 条件) 仍应告警
	if winTitleWarning("notepad.exe") == "" {
		t.Error(`winTitleWarning("notepad.exe") == "", want non-empty warning`)
	}
}

func assertEqual[T comparable](t *testing.T, a, b T) {
	t.Helper()
	if a != b {
		t.Errorf("%v != %v", a, b)
	}
}
