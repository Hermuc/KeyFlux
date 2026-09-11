// Package generators 负责把配置渲染为 AHK 代码。
// 每个动作 TypeID 一个文件 (type1~type9), 统一注册到 ActionMap。
// 依赖方向: generators -> model, 不依赖 script 包。
package generators

import (
	"fmt"
	"settings/internal/script/model"
	"sort"
	"strings"
	"text/template"
)

// Cfg 在 script.SaveAHK 中初始化, 会被下面的转换函数用到
var Cfg *model.Config

// ActionMap: 动作 TypeID -> 渲染函数。
// inAbbrContext 为 true 时渲染为缩写 switch 内的调用语句, 否则渲染为完整的 km.Map 注册行。
var ActionMap = map[int]func(model.Action, bool) string{
	1:              activateOrRun1,
	2:              systemActions2,
	3:              windowActions3,
	4:              mouseActions4,
	model.RemapKey: remapKey5,
	6:              sendKeys6,
	7:              textFeatures7,
	8:              builtinFunctions8,
	9:              keyfluxActions9,
}

func ActionToHotkey(action model.Action) string {
	if f, ok := ActionMap[action.TypeID]; ok {
		return f(action, false)
	}
	return ""
}

var TemplateFuncMap = template.FuncMap{
	"contains":            strings.Contains,
	"concat":              concat,
	"join":                join,
	"ahkString":           model.AhkString,
	"escapeAhkHotkey":     escapeAhkHotkey,
	"actionToHotkey":      ActionToHotkey,
	"abbrRegistryCode":    AbbrRegistryCode,
	"sortHotkeys":         sortHotkeys,
	"divide":              divide,
	"renderKeymap":        renderKeymap,
	"GroupDisableKeyFlux": GroupDisableKeyFlux,
	"selectedActionCode":  selectedActionCode,
}

func divide(a, b int) string {
	res := float64(a) / float64(b)
	if res <= 0 {
		return ""
	}
	return fmt.Sprintf("%.3f", res)
}

func join(sep string, elems []interface{}) string {
	elems2 := make([]string, 0)
	for _, val := range elems {
		elems2 = append(elems2, val.(string))
	}
	return strings.Join(elems2, sep)
}

func escapeAhkHotkey(key string) string {
	if key == ";" {
		return "`;"
	}
	return key
}

func concat(a, b string) string {
	return a + b
}

func sortHotkeys(hotkeyMap map[string][]model.Action) []model.Action {
	res := make([]model.Action, 0, len(hotkeyMap))
	for hotkey, actions := range hotkeyMap {
		for _, action := range actions {
			// 去掉首末的 " 字符, 虽然此处按字节取子切片也行, 但写法不够通用
			action.Hotkey = substr(model.ToAHKFuncArg(hotkey), 1, -1)
			res = append(res, action)
		}
	}

	sort.Slice(res, func(i, j int) bool {
		// 先根据 action type 排, 然后根据 hotkey 的长度和字典序来排序
		if res[i].TypeID != res[j].TypeID {
			return res[i].TypeID < res[j].TypeID
		}

		if len(res[i].Hotkey) != len(res[j].Hotkey) {
			return len(res[i].Hotkey) < len(res[j].Hotkey)
		}

		return res[i].Hotkey < res[j].Hotkey
	})

	return res
}

// 取子字符串，并不想看起来那么简单: https://stackoverflow.com/a/56129336
// NOTE: this isn't multi-Unicode-codepoint-aware, like specifying skintone or
// gender of an emoji: https://unicode.org/emoji/charts/full-unicode-modifiers.html
func substr(input string, start int, length int) string {
	asRunes := []rune(input)

	if start >= len(asRunes) {
		return ""
	}

	if start+length > len(asRunes) {
		length = len(asRunes) - start
	} else if length < 0 {
		length = len(asRunes) - start + length
	}

	return string(asRunes[start : start+length])
}

func renderKeymap(km model.Keymap) string {
	if "" == strings.TrimSpace(km.Hotkey) {
		return ""
	}
	var buf strings.Builder

	// ; Capslock + F
	buf.WriteString(fmt.Sprintf("\n  ; %s\n", km.Name))

	// km6 := KeymapManager.AddSubKeymap(km5, "*f", "Capslock + F")
	line := fmt.Sprintf("  km%d := KeymapManager.", km.ID)
	hotkey := km.Hotkey
	if containsOnlyModifier(km.Hotkey) {
		hotkey = "customHotkeys"
	}
	s := model.AhkString
	if km.ParentID == 0 {
		line += fmt.Sprintf("NewKeymap(%s, %s, %s, %s)\n", s(hotkey), s(km.Name), s(divide(km.Delay, 1000)), s(Cfg.GetKeymapDisableAt(km.ID)))
	} else {
		line += fmt.Sprintf("AddSubKeymap(km%d, %s, %s, %s)\n", km.ParentID, s(hotkey), s(km.Name), s(divide(km.Delay, 1000)))
	}
	buf.WriteString(line)

	// km := km6
	buf.WriteString(fmt.Sprintf("  km := km%d\n", km.ID))

	for _, action := range sortHotkeys(km.Hotkeys) {
		if containsOnlyModifier(km.Hotkey) {
			if action.Hotkey == "singlePress" {
				continue
			}
			action.Hotkey = km.Hotkey + action.Hotkey // 把触发键 # 和热键 *q 拼起来
		}
		buf.WriteString(fmt.Sprintf("  %s\n", ActionToHotkey(action)))
	}

	// 替换换行符为 \r\n
	res := buf.String()
	res = strings.ReplaceAll(res, "\r\n", "\n")
	res = strings.ReplaceAll(res, "\n", "\r\n")
	return res
}

func containsOnlyModifier(hotkey string) bool {
	hotkey = strings.TrimSpace(hotkey)
	return hotkey != "" && strings.Trim(hotkey, "#!^+<>*~$") == ""
}

func GroupDisableKeyFlux(groups []model.WindowGroup) string {
	for _, g := range groups {
		if g.ID == -1 {
			return model.GroupToWinTile(g)
		}
	}
	return model.AhkString("")
}
