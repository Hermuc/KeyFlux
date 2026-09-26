package generators

import (
	"fmt"
	"settings/internal/script/model"
	"strings"
)

// winTitleWarning 检测可疑的窗口标识符, 返回警告注释 (空串表示无警告)
// 裸写 "xxx.exe" 会被 AHK 当作窗口标题子串匹配, 永远匹配失败, 应写 "ahk_exe xxx.exe"
func winTitleWarning(winTitle string) string {
	if winTitle == "" || strings.HasPrefix(winTitle, "ahk_") || strings.HasPrefix(winTitle, "ahk-expression:") {
		return ""
	}
	// 组合串 "标题 ahk_exe 名.exe" 含 ahk_ 条件, 放行
	if strings.Contains(winTitle, " ahk_") {
		return ""
	}
	if strings.HasSuffix(strings.ToLower(winTitle), ".exe") {
		return `; [配置警告] winTitle "` + winTitle + `" 以 .exe 结尾, 会被当作窗口标题匹配而永远失败, 应写 "ahk_exe ` + winTitle + `"`
	}
	return ""
}

// TypeID 1: 激活或运行程序/路径
func activateOrRun1(a model.Action, inAbbrContext bool) string {
	ctx := Cfg.GetHotkeyContext(a)
	winTitle := model.ToAHKFuncArg(a.WinTitle)
	target := model.ToAHKFuncArg(a.Target)
	args := model.ToAHKFuncArg(a.Args)
	workingDir := model.ToAHKFuncArg(a.WorkingDir)

	call := fmt.Sprintf(`ActivateOrRun(%s, %s, %s, %s, %t, %t, %t)`, winTitle, target, args, workingDir, a.RunAsAdmin, a.DetectHiddenWindow, a.RunInBackground)
	if args == `""` && workingDir == `""` && !a.RunAsAdmin && !a.DetectHiddenWindow && !a.RunInBackground {
		call = fmt.Sprintf(`ActivateOrRun(%s, %s)`, winTitle, target)
	}

	if inAbbrContext {
		return call
	}

	code := fmt.Sprintf(`km.Map("%[1]s", _ => %s%s)`, a.Hotkey, call, ctx)
	if warn := winTitleWarning(a.WinTitle); warn != "" {
		return warn + "\n" + code
	}
	return code
}
