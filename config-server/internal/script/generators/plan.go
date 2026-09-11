package generators

import (
	"encoding/json"
	"os"
	"settings/internal/behaviors"
	"sort"
	"strings"

	"settings/internal/script/model"
)

// 注册计划 (Oracle 机制的 Go 侧):
// 生成端对"运行时应当注册什么"的确定性描述, 输出为 JSON (settings.exe DumpPlan)。
// 未来 AHK 运行时加载器 (bin/lib/actions) 在注册完成后导出同构计划,
// 两者 diff 为空即证明运行时行为与生成端意图一致 (零行为变更验证)。
//
// 同源约束: 计划必须由与渲染路径完全相同的函数推导 ——
// EnabledKeymaps / sortHotkeys / sortActions / ActionMap / GetWinTitle / Preprocess,
// 任何渲染规则变化都必须同步反映到这里。

// PlanVersion 计划格式版本, 结构变化时递增
const PlanVersion = 1

type Plan struct {
	PlanVersion    int                 `json:"planVersion"`
	Keymaps        []PlanKeymap        `json:"keymaps"`
	Abbr           PlanAbbr            `json:"abbr"`
	SelectedAction PlanSelectedAction  `json:"selectedAction"` // 旧 actionSchemes 段已随「单键分发」重构改造
	WindowGroups   []model.WindowGroup `json:"windowGroups"`
}

type PlanKeymap struct {
	ID        int         `json:"id"`
	Name      string      `json:"name"`
	Hotkey    string      `json:"hotkey"` // 与 renderKeymap 一致: 纯修饰键模式记为 "customHotkeys"
	ParentID  int         `json:"parentID"`
	DelaySec  string      `json:"delaySec"` // 与模板 divide(delay,1000) 输出一致
	DisableAt string      `json:"disableAt"`
	Entries   []PlanEntry `json:"entries"`
}

type PlanEntry struct {
	Hotkey        string `json:"hotkey"` // 与渲染时传给 km.Map 的热键一致 (含修饰键拼接)
	TypeID        int    `json:"typeID"`
	ValueID       int    `json:"valueID"`
	WindowGroupID int    `json:"windowGroupID"`
	ConditionType int    `json:"conditionType"`
	WinTitle      string `json:"winTitle"` // GetWinTitle 的渲染形态, AHK 端逐字接收
	Comment       string `json:"comment,omitempty"`
}

type PlanAbbr struct {
	CapslockEnabled  bool            `json:"capslockEnabled"`
	CapslockKeys     string          `json:"capslockKeys"`
	Capslock         []PlanAbbrEntry `json:"capslock"`
	SemicolonEnabled bool            `json:"semicolonEnabled"`
	SemicolonKeys    string          `json:"semicolonKeys"`
	Semicolon        []PlanAbbrEntry `json:"semicolon"`
}

type PlanAbbrEntry struct {
	Abbr    string      `json:"abbr"`
	Actions []PlanEntry `json:"actions"`
}

// PlanSelectedAction 选中动作单键分发的计划投影, 与 selectedActionCode 渲染形态镜像:
// 同一过滤口径 (enable && hotkey 非空才注册; 零 mappings / 超 key cap 的 entry 同口径跳过)、
// 同一 ResolveRuleAction 展开与显示名回退。
type PlanSelectedAction struct {
	Hotkey   string                `json:"hotkey"`
	Enable   bool                  `json:"enable"`
	Mappings []PlanSelectedMapping `json:"mappings"` // 空时恒输出 []
}

type PlanSelectedMapping struct {
	MatchType  string              `json:"matchType"`
	MatchValue string              `json:"matchValue"`
	Entries    []PlanSelectedEntry `json:"entries"` // 空时恒输出 []
}

// PlanSelectedEntry 字段含义与 selectedActionCode 数据数组一致 (key=菜单序号 1-9)。
type PlanSelectedEntry struct {
	Key         int    `json:"key"`
	Behavior    string `json:"behavior"`
	Action      string `json:"action"`
	ActionValue string `json:"actionValue"`
	WorkingDir  string `json:"workingDir"`
	Name        string `json:"name"`
}

// BuildPlan 推导注册计划。调用方必须先执行 script.Preprocess (注入 !f17),
// 与 GenerateAHK/GenerateScripts 路径保持一致。
func BuildPlan(cfg *model.Config) *Plan {
	return &Plan{
		PlanVersion:    PlanVersion,
		Keymaps:        planKeymaps(cfg),
		Abbr:           planAbbr(cfg),
		SelectedAction: planSelectedAction(cfg),
		WindowGroups:   cfg.Options.WindowGroups,
	}
}

// WritePlan 生成计划并写入 JSON 文件 (2 空格缩进, 字段顺序由结构体固定, 输出确定性)
func WritePlan(cfg *model.Config, outputFile string) error {
	data, err := json.MarshalIndent(BuildPlan(cfg), "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(outputFile, append(data, '\n'), 0644)
}

func planKeymaps(cfg *model.Config) []PlanKeymap {
	var res []PlanKeymap
	for _, km := range cfg.EnabledKeymaps() {
		// 与 renderKeymap 一致: 空白热键的模式不渲染
		if "" == strings.TrimSpace(km.Hotkey) {
			continue
		}
		hotkey := km.Hotkey
		if containsOnlyModifier(km.Hotkey) {
			hotkey = "customHotkeys"
		}
		res = append(res, PlanKeymap{
			ID:        km.ID,
			Name:      km.Name,
			Hotkey:    hotkey,
			ParentID:  km.ParentID,
			DelaySec:  divide(km.Delay, 1000),
			DisableAt: cfg.GetKeymapDisableAt(km.ID),
			Entries:   planEntries(cfg, km),
		})
	}
	return res
}

func planEntries(cfg *model.Config, km model.Keymap) []PlanEntry {
	var entries []PlanEntry
	for _, action := range deterministicSort(sortHotkeys(km.Hotkeys)) {
		// 与 renderKeymap 一致: 纯修饰键模式跳过 singlePress, 其余拼接触发键
		if containsOnlyModifier(km.Hotkey) {
			if action.Hotkey == "singlePress" {
				continue
			}
			action.Hotkey = km.Hotkey + action.Hotkey
		}
		// 与 ActionToHotkey 一致: 未注册的 TypeID 不产生注册
		if _, ok := ActionMap[action.TypeID]; !ok {
			continue
		}
		entries = append(entries, toPlanEntry(cfg, action))
	}
	return entries
}

func planAbbr(cfg *model.Config) PlanAbbr {
	res := PlanAbbr{
		Capslock:         []PlanAbbrEntry{},
		Semicolon:        []PlanAbbrEntry{},
		CapslockEnabled:  cfg.CapslockAbbrEnabled(),
		SemicolonEnabled: cfg.SemicolonAbbrEnabled(),
	}
	// 与模板一致: 仅启用的缩写表会被渲染注册
	if res.CapslockEnabled {
		res.CapslockKeys = cfg.CapslockAbbrKeys()
		res.Capslock = planAbbrEntries(cfg, cfg.CapslockAbbr())
	}
	if res.SemicolonEnabled {
		res.SemicolonKeys = cfg.SemicolonAbbrKeys()
		res.Semicolon = planAbbrEntries(cfg, cfg.SemicolonAbbr())
	}
	return res
}

func planAbbrEntries(cfg *model.Config, abbrMap map[string][]model.Action) []PlanAbbrEntry {
	// 与 AbbrRegistryCode 一致: 按缩写字典序, 动作经 sortActions, 跳过未注册 TypeID
	type Abbr struct {
		abbr    string
		actions []model.Action
	}
	var abbrList []Abbr
	for abbr, actions := range abbrMap {
		abbrList = append(abbrList, Abbr{abbr, sortActions(actions)})
	}
	sort.Slice(abbrList, func(i, j int) bool {
		return abbrList[i].abbr < abbrList[j].abbr
	})

	res := make([]PlanAbbrEntry, 0, len(abbrList))
	for _, item := range abbrList {
		entry := PlanAbbrEntry{Abbr: item.abbr, Actions: []PlanEntry{}}
		for _, a := range item.actions {
			if _, ok := ActionMap[a.TypeID]; !ok {
				continue
			}
			entry.Actions = append(entry.Actions, toPlanEntry(cfg, a))
		}
		res = append(res, entry)
	}
	return res
}

func planSelectedAction(cfg *model.Config) PlanSelectedAction {
	// 与 selectedActionCode 一致: 仅 enable 且 hotkey 非空才注册热键, 禁用/空热键输出
	// 空结构 (mappings 恒 [])。评审 L5: 零 mappings 时 Hotkey 也记空串 —— AHK 端
	// SelectedActionInit 对 entries.Length==0 早退不注册热键, 计划必须同口径,
	// 否则 Oracle 对账出现「计划有热键、运行时无注册」的假阳性。
	res := PlanSelectedAction{Mappings: []PlanSelectedMapping{}}
	sa := cfg.SelectedAction
	if sa == nil || !sa.Enable || sa.Hotkey == "" {
		return res
	}
	res.Hotkey = sa.Hotkey
	res.Enable = sa.Enable
	for _, m := range sa.Mappings {
		pm := PlanSelectedMapping{
			MatchType:  m.MatchType,
			MatchValue: m.MatchValue,
			Entries:    []PlanSelectedEntry{},
		}
		for i, e := range m.Entries {
			// 评审 L2: 与 selectedActionCode 同口径, 超 key cap 的 entry 不进计划
			if i+1 > selectedActionKeyCap {
				continue
			}
			// 与 selectedActionCode 一致: 计划记录解析后的实际注册值 (内置 ID 直通,
			// 用户行为展开为基础动作+包默认模板), 保证 Oracle 计划与运行时一致
			action, actionValue, workingDir := behaviors.ResolveRuleAction(BehaviorCatalog, e.Behavior, e.ActionValue, e.WorkingDir)
			pm.Entries = append(pm.Entries, PlanSelectedEntry{
				Key:         i + 1,
				Behavior:    e.Behavior,
				Action:      action,
				ActionValue: actionValue,
				WorkingDir:  workingDir,
				Name:        behaviorName(e.Behavior),
			})
		}
		res.Mappings = append(res.Mappings, pm)
	}
	if len(res.Mappings) == 0 {
		// 评审 L5: 零 mappings 与渲染器/AHK 端「空串不注册」口径统一
		res.Hotkey = ""
	}
	return res
}

func toPlanEntry(cfg *model.Config, a model.Action) PlanEntry {
	winTitle, conditionType := cfg.GetWinTitle(a)
	// 与 AbbrRegistryCode 一致: conditionType 5 的表达式去掉包裹的单引号
	if conditionType == 5 {
		winTitle = strings.Trim(winTitle, "'")
	}
	return PlanEntry{
		Hotkey:        a.Hotkey,
		TypeID:        a.TypeID,
		ValueID:       a.ValueID,
		WindowGroupID: a.WindowGroupID,
		ConditionType: conditionType,
		WinTitle:      winTitle,
		Comment:       a.Comment,
	}
}

// deterministicSort 在 sortHotkeys 的基础上追加 windowGroupID 兜底,
// 保证同热键同类型的多动作 (不同窗口组) 输出顺序稳定, 计划可逐字节复现。
func deterministicSort(actions []model.Action) []model.Action {
	sort.SliceStable(actions, func(i, j int) bool {
		if actions[i].TypeID != actions[j].TypeID {
			return actions[i].TypeID < actions[j].TypeID
		}
		if len(actions[i].Hotkey) != len(actions[j].Hotkey) {
			return len(actions[i].Hotkey) < len(actions[j].Hotkey)
		}
		if actions[i].Hotkey != actions[j].Hotkey {
			return actions[i].Hotkey < actions[j].Hotkey
		}
		return actions[i].WindowGroupID < actions[j].WindowGroupID
	})
	return actions
}
