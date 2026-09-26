package generators

import (
	"fmt"
	"settings/internal/script/model"
	"sort"
	"strings"
)

// TODO 缩写功能中不支持的操作:
// 重映射按键, 锁定当前模式, 暂停重启锁定, 鼠标操作, 窗口操作, ....
// 基本上只支持发送按键, 启动程序, 执行内置函数, 系统控制
//
// AbbrRegistryCode 把缩写表渲染为 CommandResolver.Register 注册行 (阶段 4)。
// 按缩写字典序, 动作经 sortActions, 跳过未注册 TypeID,
// 仅 WindowGroupID != 0 的动作带窗口组守卫, conditionType 5 表达式去包裹单引号。
// 闭包体即原 switch case 体语句, 参数在执行时求值, 语义不变。
// 注册行输出在 InitKeymap 的路径变量之后, 闭包可捕获路径变量。
// indent 为每行的公共前缀缩进。
func AbbrRegistryCode(abbrMap map[string][]model.Action, scope, indent string) string {
	type Abbr struct {
		abbr    string
		actions []model.Action
	}
	var abbrList []Abbr
	for abbr, actions := range abbrMap {
		actions = sortActions(actions)
		abbrList = append(abbrList, Abbr{abbr, actions})
	}
	sort.Slice(abbrList, func(i, j int) bool {
		return abbrList[i].abbr < abbrList[j].abbr
	})

	var s strings.Builder
	for _, abbr := range abbrList {
		var steps []string
		for _, a := range abbr.actions {
			if _, ok := ActionMap[a.TypeID]; !ok {
				continue
			}

			call := ActionMap[a.TypeID](a, true)
			step := fmt.Sprintf("CommandStep(() => %s)", call)
			if a.WindowGroupID != 0 {
				winTitle, conditionType := Cfg.GetWinTitle(a)
				if conditionType == 5 {
					winTitle = strings.Trim(winTitle, "'")
				}
				step = fmt.Sprintf("CommandStep(() => %s, %s, %d)", call, winTitle, conditionType)
			}
			steps = append(steps, step)
		}
		if len(steps) == 0 {
			continue
		}
		s.WriteString(fmt.Sprintf("%sCommandResolver.Register(%s, %s, [%s])\n",
			indent, model.AhkString(scope), model.AhkString(abbr.abbr), strings.Join(steps, ", ")))
	}
	return s.String()
}
