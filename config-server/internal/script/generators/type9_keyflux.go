package generators

import (
	"fmt"
	"strings"

	"settings/internal/script/model"
)

// TypeID 9: KeyFlux 自身动作 (暂停/重载/退出/设置/进缩写模式/大写锁定/锁定)
// + 插件动作 (P7b: actionId = "<pluginId>.<actionId>", valueID 9 = 子类型标记)。
func keyfluxActions9(a model.Action, inAbbrContext bool) string {
	ctx := Cfg.GetHotkeyContext(a)

	// P7b: 插件动作绑定 (双字段过渡 —— actionId 渲染优先; valueID 9 继续写,
	// 供旧版本识别「插件动作」子类型)。核心对具体插件零知识: 只按 "." 拆分转发,
	// 寻址由运行时动作注册表完成 (插件缺席即静默失败, 可删除性保证)。
	if a.ActionID != "" {
		pluginID, actionID, ok := strings.Cut(a.ActionID, ".")
		if !ok {
			return ""
		}
		call := fmt.Sprintf("PluginAction(%s, %s)", ahkStringLit(pluginID), ahkStringLit(actionID))
		if inAbbrContext {
			return call
		}
		return fmt.Sprintf(`km.Map("%[1]s", _ => %s%s)`, a.Hotkey, call, ctx)
	}

	callMap := map[int]string{
		1: `KeyFluxToggleSuspend()`,
		2: `KeyFluxReload()`,
		3: `KeyFluxExit()`,
		4: `KeyFluxOpenSettings()`,
		5: `EnterSemicolonAbbr(semiHook, semiHookAbbrWindow)`,
		6: `EnterCapslockAbbr()`,
		7: `ToggleCapslock()`,
		8: `km.ToggleLock`,
	}

	call, ok := callMap[a.ValueID]
	if !ok {
		// 旧式 valueID 9 且无 actionId 的遗留绑定: 生成端无法定位插件动作, 跳过
		// (出厂配置 2026-09-23 起已无此绑定; 用户重新绑定即写入 actionId)。
		return ""
	}

	if inAbbrContext {
		return call
	}

	if a.ValueID == 1 || a.ValueID == 2 {
		if ctx == "" {
			ctx += ", , , "
		}
		return fmt.Sprintf(`km.Map("%[1]s", _ => %s%s, "S")`, a.Hotkey, call, ctx)
	}
	if a.ValueID == 8 {
		return fmt.Sprintf(`km.Map("%[1]s", %s%s)`, a.Hotkey, call, ctx)
	}

	return fmt.Sprintf(`km.Map("%[1]s", _ => %s%s)`, a.Hotkey, call, ctx)
}
