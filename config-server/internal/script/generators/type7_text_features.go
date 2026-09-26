package generators

import (
	"fmt"
	"settings/internal/script/model"
)

// TypeID 7: 文本编辑特征键 (方向/选区/重映射变体)
func textFeatures7(a model.Action, inAbbrContext bool) string {

	m := map[int]struct {
		Type  string
		Value string
	}{
		1:  {"remap", "up"},
		2:  {"remap", "down"},
		3:  {"remap", "left"},
		4:  {"remap", "right"},
		5:  {"remap", "home"},
		6:  {"remap", "end"},
		17: {"remap", "appskey"},
		20: {"remap", "esc"},
		21: {"remap", "backspace"},
		23: {"remap", "delete"},
		24: {"remap", "insert"},
		25: {"remap", "tab"},

		7:  {"send", "{blind}^{left}"},
		8:  {"send", "{blind}^{right}"},
		9:  {"send", "{blind}+{up}"},
		10: {"send", "{blind}+{down}"},
		11: {"send", "{blind}+{left}"},
		12: {"send", "{blind}+{right}"},
		13: {"send", "{blind}+{home}"},
		14: {"send", "{blind}+{end}"},
		15: {"send", "^+{left}"},
		16: {"send", "^+{right}"},
		18: {"send", "^{backspace}"},
		33: {"send", "{home}+{end}{backspace}"},
		22: {"send", "{blind}{enter}"},
		26: {"send", "^{tab}"},
		27: {"send", "{blind}+{tab}"},
		28: {"send", "^+{tab}"},
	}

	if item, ok := m[a.ValueID]; ok {
		if item.Type == "remap" {
			a.RemapToKey = item.Value
			return remapKey5(a, inAbbrContext)
		}
		if item.Type == "send" {
			a.KeysToSend = item.Value
			return sendKeys6(a, inAbbrContext)
		}
	}

	callMap := map[int]string{
		19: `HoldDownModifierKey("LShift")`,
		29: `InsertSpaceBetweenZHAndEn()`,
		30: `HoldDownModifierKey("LCtrl")`,
		31: `HoldDownModifierKey("LAlt")`,
		32: `HoldDownModifierKey("LWin")`,
	}
	if call, ok := callMap[a.ValueID]; ok {
		if inAbbrContext {
			return call
		}
		return fmt.Sprintf(`km.Map("%[1]s", _ => %s%s)`, a.Hotkey, call, Cfg.GetHotkeyContext(a))
	}
	return ""
}
