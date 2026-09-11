package generators

import (
	"fmt"
	"strings"

	"settings/internal/behaviors"
	"settings/internal/script/model"
)

// BehaviorCatalog 渲染期行为目录, 由调用方注入 (script.GenerateScripts / command.GenerateAHK /
// command.DumpPlan)。为 nil 或 entry 引用内置基础动作 ID 时直通 —— golden 快照与既有配置
// 的 DumpPlan 因此逐字节稳定。
var BehaviorCatalog *behaviors.Catalog

// selectedActionKeyCap 单 mapping 菜单序号上限 (与 script.maxEntriesPerMapping 同口径;
// generators 不 import script 避免依赖环, 修改时两处同步; plan.go 投影同口径引用)。
const selectedActionKeyCap = 9

// selectedActionCode 把 selectedAction (单键分发) 渲染为 AHK 代码 (用于 keyflux.tmpl 模板)。
// 产物由 bin/lib/rules/SelectedAction.ahk 的 SelectedActionInit 消费 (任务 #30 按此实现):
// 单热键注册 + 数据数组; 数组顺序 = mappings 配置顺序 (即匹配优先级), 同一 mapping 内
// entries 顺序 = 菜单序号 1..n。
//
// 数据数组每项字段含义 (稳定可 diff, 供 SelectedAction.ahk 消费):
//
//	matchType:  匹配类型 "fileExt" | "textType"
//	matchValue: 匹配条件值 (fileExt=逗号分隔后缀; textType=url/path/magnet/plain)
//	key:        菜单序号 1-9 (同一 mapping 内从 1 递增, 用户按数字键选择)
//	behavior:   行为库 ID (config.json selectedAction 原值, UI 展示用)
//	action:     ResolveRuleAction 展开后的实际基础动作 (内置 ID 直通, 用户包 entry.action)
//	actionValue:展开后的动作模板 (entry 非空覆盖包默认, 空补包默认)
//	workingDir: 展开后的工作目录 (同上)
//	name:       行为显示名 (包名, 供菜单展示; 目录缺失时回退 behavior ID)
//
// 注: entry.options (确认/复制/清空) 本版不进数据数组 —— 方案 D 菜单语义下其行为
// 未定义, 由任务 #30 的 AHK 端决定是否消费。
// 禁用 / 热键为空 / 未配置 (nil) 时输出空串, 不注册任何热键 (与旧 actionSchemesCode 的
// 跳过口径一致, 保证出厂默认配置产物逐字节稳定)。
// 评审 L2: 同一 mapping 内 key 递增超过 9 的 entry 跳过渲染并留注释告警 ——
// 保存校验 (script.ValidateSelectedAction) 已拒绝 >9, 此处防御手改配置漏检;
// AHK 端 _RunMenu 对 1..9 之外的 key 同样跳过, 三层同口径。
func selectedActionCode(sa *model.SelectedAction) string {
	if sa == nil || !sa.Enable || sa.Hotkey == "" {
		return ""
	}

	var buf strings.Builder
	buf.WriteString("  ; ===== 选中动作 (单键分发) =====\n")
	buf.WriteString("  ; SelectedActionData 每项字段: matchType (匹配类型, 输出顺序 = 匹配优先级) / matchValue (条件值) / key (菜单序号 1-9) /\n")
	buf.WriteString("  ;   behavior (行为库 ID) / action (展开后基础动作) / actionValue (展开后模板) / workingDir (工作目录) / name (显示名)\n")
	buf.WriteString("  SelectedActionData := Array(\n")
	for _, m := range sa.Mappings {
		for i, e := range m.Entries {
			if i+1 > selectedActionKeyCap {
				// 评审 L2: 超 key cap 的 entry 跳过渲染, 留注释告警供产物 diff 排查
				buf.WriteString("    ; [selected-action] entry beyond key cap 9 skipped\n")
				continue
			}
			// 行为 ID 解析: 内置动作直通; 用户包 builtin entry 展开为基础动作+包默认模板
			// (entry 非空值优先, 空值补包默认); 与 plan.go planSelectedAction 的镜像逻辑一致
			action, actionValue, workingDir := behaviors.ResolveRuleAction(BehaviorCatalog, e.Behavior, e.ActionValue, e.WorkingDir)
			buf.WriteString(fmt.Sprintf("    {matchType: %s, matchValue: %s, key: %d, behavior: %s, action: %s, actionValue: %s, workingDir: %s, name: %s},\n",
				model.AhkString(m.MatchType), model.AhkString(m.MatchValue), i+1,
				model.AhkString(e.Behavior), model.AhkString(action),
				model.AhkString(actionValue), model.AhkString(workingDir),
				model.AhkString(behaviorName(e.Behavior))))
		}
	}
	buf.WriteString("  )\n")
	buf.WriteString(fmt.Sprintf("  SelectedActionInit(%s, SelectedActionData)\n", model.AhkString(sa.Hotkey)))
	return buf.String()
}

// behaviorName 行为显示名: 优先包名, 目录缺失 (golden 测试/纯 CLI) 回退原始 ID。
func behaviorName(id string) string {
	if BehaviorCatalog != nil {
		if p := BehaviorCatalog.Get(id); p != nil {
			return p.Name
		}
	}
	return id
}
