package generators

import (
	"fmt"
	"settings/internal/script/model"
	"strings"
)

// TypeID 5: 重映射按键
func remapKey5(a model.Action, inAbbrContext bool) string {
	if strings.ToLower(a.Hotkey) == "singlepress" {
		a.KeysToSend = "{blind}{" + a.RemapToKey + "}"
		return sendKeys6(a, inAbbrContext)
	}
	key := strings.TrimLeft(a.Hotkey, "*")
	ctx := Cfg.GetHotkeyContext(a)
	if ctx != "" {
		ctx = ctx[2:]
	}
	f := "RemapKey"
	if a.RemapInHotIf {
		f = "RemapInHotIf"
	}
	return fmt.Sprintf(`km.%s("%s", %s%s)`, f, key, model.ToAHKFuncArg(a.RemapToKey), ctx)
}
