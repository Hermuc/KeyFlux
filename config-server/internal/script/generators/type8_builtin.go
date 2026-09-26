package generators

import (
	"fmt"
	"settings/internal/script/model"
)

// TypeID 8: 内置函数/自定义 AHK 表达式 (遗留代码载荷, 由 Go 编译通道直出)
func builtinFunctions8(a model.Action, inAbbrContext bool) string {
	if inAbbrContext {
		return a.AHKCode
	}
	return fmt.Sprintf(`km.Map("%[1]s", _ => %s%s)`, a.Hotkey, a.AHKCode, Cfg.GetHotkeyContext(a))
}
