package generators

import (
	"fmt"
	"settings/internal/script/model"
	"strings"
)

// TypeID 6: 发送按键 (支持多行、ahk: 行、sleep 行)
func sendKeys6(a model.Action, inAbbrContext bool) string {
	// 有多行, 每行转成一个 send 调用, 然后用逗号 , 连起来
	var res []string
	lines := strings.Split(a.KeysToSend, "\n")
	for _, line := range lines {
		if len(strings.TrimSpace(line)) == 0 {
			continue
		}
		if strings.HasPrefix(line, "ahk:") {
			res = append(res, strings.TrimSpace(line[4:]))
			continue
		}
		if strings.HasPrefix(line, "sleep ") || strings.HasPrefix(line, "Sleep ") {
			res = append(res, fmt.Sprintf(`Sleep(%s)`, line[6:]))
			continue
		}
		line = model.ToAHKFuncArg(line)
		res = append(res, fmt.Sprintf(`Send(%s)`, line))
	}
	if len(res) == 0 {
		return ""
	}

	call := strings.Join(res, ", ")

	if inAbbrContext {
		return call
	}
	return fmt.Sprintf(`km.Map("%[1]s", _ => (%s)%s)`, a.Hotkey, call, Cfg.GetHotkeyContext(a))
}
