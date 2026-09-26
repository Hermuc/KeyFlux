package generators

import (
	"fmt"
	"settings/internal/script/model"
)

// TypeID 2: 系统控制动作
func systemActions2(a model.Action, inAbbrContext bool) string {
	callMap := map[int]string{
		1:  `SystemLockScreen()`,
		2:  `SystemSleep()`,
		3:  `SystemShutdown()`,
		4:  `SystemReboot()`,
		5:  `SoundControl()`,
		6:  `BrightnessControl()`,
		7:  `SystemRestartExplorer()`,
		8:  `CopySelectedAsPlainText()`,
		9:  `MuteActiveApp()`,
		10: `ShowActiveProcessInFolder()`,
	}
	if call, ok := callMap[a.ValueID]; ok {
		if inAbbrContext {
			return call
		}
		return fmt.Sprintf(`km.Map("%[1]s", _ => %s%s)`, a.Hotkey, call, Cfg.GetHotkeyContext(a))
	}
	return ""
}
