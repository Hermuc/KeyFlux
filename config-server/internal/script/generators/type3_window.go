package generators

import (
	"fmt"
	"settings/internal/script/model"
)

// TypeID 3: 窗口操作
func windowActions3(a model.Action, inAbbrContext bool) string {
	if a.ValueID == 4 {
		return fmt.Sprintf(`km.Map("%[1]s", _ => Send("^!{tab}"), taskSwitch%s)`, a.Hotkey, Cfg.GetHotkeyContext(a))
	}
	if a.ValueID == 14 {
		return fmt.Sprintf(`km.Map("%[1]s", BindWindow()%s)`, a.Hotkey, Cfg.GetHotkeyContext(a))
	}
	callMap := map[int]string{
		1:  `SmartCloseWindow()`,
		2:  `GoToLastWindow()`,
		3:  `LoopRelatedWindows()`,
		5:  `GoToPreviousVirtualDesktop()`,
		6:  `GoToNextVirtualDesktop()`,
		7:  `MoveWindowToNextMonitor()`,
		8:  `MinimizeWindow()`,
		9:  `MaximizeWindow()`,
		10: `CenterAndResizeWindow(1200, 800)`,
		11: `CenterAndResizeWindow(1370, 930)`,
		12: `ToggleWindowTopMost()`,
		13: `MakeWindowDraggable()`,
		15: `CloseWindowProcesses()`,
		16: `CloseSameClassWindows()`,
	}
	if call, ok := callMap[a.ValueID]; ok {
		if inAbbrContext {
			return call
		}
		return fmt.Sprintf(`km.Map("%[1]s", _ => %s%s)`, a.Hotkey, call, Cfg.GetHotkeyContext(a))
	}
	return ""
}
