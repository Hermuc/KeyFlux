package generators

import (
	"settings/internal/script/model"
)

func sortActions(actions []model.Action) []model.Action {
	// 把 a.WindowGroupID 为 0 的放在最后面, 因为这是默认生效的动作, 优先级最低
	res := make([]model.Action, 0, len(actions))
	var suffix []model.Action
	for _, a := range actions {
		if a.WindowGroupID == 0 {
			suffix = append(suffix, a)
		} else {
			res = append(res, a)
		}
	}
	res = append(res, suffix...)
	return res
}
