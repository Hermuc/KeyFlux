package generators

import (
	"fmt"
	"settings/internal/script/model"
)

// TypeID 4: 鼠标动作 (快慢双速注册)
func mouseActions4(a model.Action, inAbbrContext bool) string {
	winTitle, conditionType := Cfg.GetWinTitle(a)
	ctx := Cfg.GetHotkeyContext(a)
	valueMap := map[int]string{
		1: `km.Map("%[1]s", fast.MoveMouseUp, slow%[2]s), slow.Map("%[1]s", slow.MoveMouseUp%[3]s)`,
		2: `km.Map("%[1]s", fast.MoveMouseDown, slow%[2]s), slow.Map("%[1]s", slow.MoveMouseDown%[3]s)`,
		3: `km.Map("%[1]s", fast.MoveMouseLeft, slow%[2]s), slow.Map("%[1]s", slow.MoveMouseLeft%[3]s)`,
		4: `km.Map("%[1]s", fast.MoveMouseRight, slow%[2]s), slow.Map("%[1]s", slow.MoveMouseRight%[3]s)`,

		5: `km.Map("%[1]s", fast.ScrollWheelUp%s), slow.Map("%[1]s", slow.ScrollWheelUp%s)`,
		6: `km.Map("%[1]s", fast.ScrollWheelDown%s), slow.Map("%[1]s", slow.ScrollWheelDown%s)`,
		7: `km.Map("%[1]s", fast.ScrollWheelLeft%s), slow.Map("%[1]s", slow.ScrollWheelLeft%s)`,
		8: `km.Map("%[1]s", fast.ScrollWheelRight%s), slow.Map("%[1]s", slow.ScrollWheelRight%s)`,

		9:  `km.Map("%[1]s", fast.LButton()%s), slow.Map("%[1]s", slow.LButton()%s)`,
		10: `km.Map("%[1]s", fast.RButton()%s), slow.Map("%[1]s", slow.RButton()%s)`,
		11: `km.Map("%[1]s", fast.MButton()%s), slow.Map("%[1]s", slow.MButton()%s)`,
		12: `km.Map("%[1]s", fast.LButtonDown()%s), slow.Map("%[1]s", slow.LButtonDown()%s)`,
		13: `km.Map("%[1]s", _ => MoveMouseToCaret()%s), slow.Map("%[1]s", _ => MoveMouseToCaret()%s)`,
	}
	if format, ok := valueMap[a.ValueID]; ok {
		if a.ValueID >= 5 {
			return fmt.Sprintf(format, a.Hotkey, ctx)
		}
		fastSuffix := fmt.Sprintf(", %s, %d", winTitle, conditionType)
		slowSuffix := fmt.Sprintf(", , %s, %d", winTitle, conditionType)
		if winTitle == `""` && conditionType == 0 {
			fastSuffix = ""
			slowSuffix = ""
		}
		return fmt.Sprintf(format, a.Hotkey, fastSuffix, slowSuffix)
	}
	return ""
}
