package script

import (
	"fmt"
	"strings"

	"settings/internal/behaviors"
)

// 选中动作单键分发 (方案 D): config.json 顶层 selectedAction 字段的迁移/校验/匹配。
// 旧多方案 actionSchemes 于 2026-09 重构为单键分发; 本文件承载三个职责:
//   - MigrateSelectedAction: 存量 actionSchemes 读时一次性投影 (硬切不回写)
//   - ValidateSelectedAction: PUT /config 保存链路的组合合法性校验
//   - MatchSelectedAction: 模拟测试 API 的匹配层 (按 mappings 顺序, 复用 matchActionRule)

// mappingKey mapping 合并去重的键: 同 matchType+matchValue 的规则归并到同一 mapping (entries 顺次追加)。
func mappingKey(matchType, matchValue string) string {
	return matchType + "|" + matchValue
}

// MigrateSelectedAction 把存量 actionSchemes 一次性投影为 selectedAction (读时迁移, 硬切不回写)。
// 规则 (定稿):
//   - 配置中已有 selectedAction 键 → 直接采用新契约, actionSchemes 忽略;
//   - 旧 actionSchemes 首个方案 hotkey/enable → selectedAction.hotkey/enable (多方案取首个);
//   - 每条 rule (matchType/matchValue/actionType/actionValue/options/workingDir) →
//     对应类型 mapping 的 entry (behavior=actionType); 同 matchType+matchValue 的 mapping
//     合并去重 (合并后 entries 顺次追加);
//   - 无 actionSchemes / 空方案 (出厂默认) → selectedAction{hotkey:"", enable:false, mappings:[]};
//   - 迁移后 ActionSchemes 置 nil: save 序列化不再输出 actionSchemes, 旧段彻底消失。
func MigrateSelectedAction(cfg *Config) {
	if cfg.SelectedAction != nil {
		cfg.ActionSchemes = nil // 新契约已存在: 旧段废弃不再输出
		return
	}
	sa := &SelectedAction{Mappings: []SelectedMapping{}}
	if len(cfg.ActionSchemes) > 0 {
		first := cfg.ActionSchemes[0]
		sa.Hotkey = first.Hotkey
		sa.Enable = first.Enable
		index := map[string]int{}
		for _, s := range cfg.ActionSchemes {
			for _, r := range s.Rules {
				key := mappingKey(r.MatchType, r.MatchValue)
				mi, ok := index[key]
				if !ok {
					sa.Mappings = append(sa.Mappings, SelectedMapping{
						MatchType:  r.MatchType,
						MatchValue: r.MatchValue,
						Entries:    []SelectedEntry{},
					})
					mi = len(sa.Mappings) - 1
					index[key] = mi
				}
				sa.Mappings[mi].Entries = append(sa.Mappings[mi].Entries, SelectedEntry{
					Behavior:    r.ActionType,
					ActionValue: r.ActionValue,
					WorkingDir:  r.WorkingDir,
					Options:     r.Options,
				})
			}
		}
	}
	cfg.SelectedAction = sa
	cfg.ActionSchemes = nil
}

// maxEntriesPerMapping 单个 mapping 的 entries 上限 (菜单序号 1-9)。
const maxEntriesPerMapping = 9

// ValidateSelectedAction 校验 selectedAction 组合合法性 (PUT /config 保存链路):
//  1. 每条 entry 的 behavior 必须在行为目录且 appliesTo 覆盖该 mapping 的类型与值
//     (复用 behaviors.Covers 语义; cat 为 nil 时跳过覆盖检查, 镜像旧 ValidateActionSchemeRules
//     对目录缺失/异常部署的容忍, 不把纯 CLI 场景锁死);
//  2. mapping 组合键 (matchType, matchValue) 不重复 —— 单键分发下重复 mapping 语义等价于
//     合并 (MigrateSelectedAction 同键归并), UI 不会产生; 手改配置出现重复时拒绝, 避免优先级歧义;
//  3. 每 mapping entries >= 1;
//  4. entries <= 9 (菜单序号 1-9);
//  5. 条件值合法性: textType 须在词表内, fileExt 归一化后非空 (评审 L1);
//  6. hotkey 非空 —— 仅启用态要求 (沿用现有渲染口径: 禁用/空热键方案不注册热键,
//     空方案/出厂默认态 enable=false 可保存, 不把存量空配置锁死在 PUT /config)。
func ValidateSelectedAction(sa *SelectedAction, cat *behaviors.Catalog, cfg *Config) error {
	if sa == nil {
		return nil
	}
	if sa.Enable && strings.TrimSpace(sa.Hotkey) == "" {
		return fmt.Errorf("选中动作已启用但热键为空, 请设置热键或关闭启用")
	}
	seen := make(map[string]bool, len(sa.Mappings))
	for i := range sa.Mappings {
		m := &sa.Mappings[i]
		// M1: 同 (matchType, matchValue) 的重复 mapping 拒绝 (大小写/首尾空白归一后比较,
		// 与运行时匹配的忽略大小写语义一致; 错误信息含重复值)
		key := strings.ToLower(m.MatchType) + "|" + strings.ToLower(strings.TrimSpace(m.MatchValue))
		if seen[key] {
			return fmt.Errorf("%s「%s」的映射条件重复, 请合并为同一映射", matchTypeName(m.MatchType), m.MatchValue)
		}
		seen[key] = true
		if len(m.Entries) == 0 {
			return fmt.Errorf("%s「%s」没有行为, 请至少添加一个行为", matchTypeName(m.MatchType), m.MatchValue)
		}
		if len(m.Entries) > maxEntriesPerMapping {
			return fmt.Errorf("%s「%s」的行为超过 %d 个 (菜单序号仅支持 1-%d)", matchTypeName(m.MatchType), m.MatchValue, maxEntriesPerMapping, maxEntriesPerMapping)
		}
		// 解析匹配前提值集 (含 type: 引用): 引用悬空或条件非法即拒绝;
		// 非 type: 值原样透传 (内置 4 文本特征走 KnownTextTypes, 文件后缀走 RefValues)。
		matchType, values, ok := ResolveMappingValues(cfg, m)
		if !ok {
			// 失败原因分两类, 分别给出可执行的提示 (合并成一句会让两种场景都失去指向性);
			// 文案口径: 面向非专业用户, 用词与设置界面「匹配类型」弹窗一致 (术语表见实施计划 §8.1)。
			if isCustomRef(m.MatchValue) {
				return fmt.Errorf("未找到引用的匹配类型「%s」，请先在「匹配类型」中创建", strings.TrimPrefix(m.MatchValue, "type:"))
			}
		if m.MatchType == "textType" {
			return fmt.Errorf("未知的文本特征「%s」，可选：%s（或在「匹配类型」中自定义）",
				m.MatchValue, behaviors.TextFeatureHint())
		}
			return fmt.Errorf("文件扩展名「%s」无效，请填写如 jpg,png（或 * 表示任意文件）", m.MatchValue)
		}
		// L1: 文件扩展名归一化后为空拒绝 (空串/纯点/纯逗号), 镜像 textType 词表拒绝风格
		if len(values) == 0 {
			return fmt.Errorf("文件扩展名「%s」无效，请填写如 jpg,png（或 * 表示任意文件）", m.MatchValue)
		}
		for j := range m.Entries {
			e := &m.Entries[j]
			// 内置基础动作在目录缺失 (异常部署/纯 CLI 场景) 时跳过覆盖检查, 保持旧配置可保存
			if cat == nil || (behaviors.BuiltinActionIDs[e.Behavior] && cat.Get(e.Behavior) == nil) {
				continue
			}
			if !cat.Covers(e.Behavior, matchType, values) {
				return fmt.Errorf("%s「%s」第 %d 项: 动作「%s」与该匹配条件不匹配",
					matchTypeName(m.MatchType), m.MatchValue, j+1, behaviorDisplayName(cat, e.Behavior))
			}
		}
	}
	return nil
}

// ResolveMappingValues 解析映射的匹配前提为 (matchType, values, ok), 是引用解析的唯一入口,
// 供生成端/校验/模拟测试共用, 禁止三处各写一份。
//   - type: 引用: 文本侧查 FindMatchType(kind=text) 返回 values=[matchValue] (供 two-segment 覆盖判定;
//     值形如 type:<id>, 既支持"专属包 value=type:<id>"也支持"plain 继承覆盖");
//     文件侧查 FileGroupExts 返回该组后缀集。任一找不到或为空即 ok=false。
//   - 非引用: fileExt 经 RefValues 展开后缀集, textType 经 KnownTextTypes 校验后单值返回。
func ResolveMappingValues(cfg *Config, m *SelectedMapping) (string, []string, bool) {
	if isCustomRef(m.MatchValue) {
		id := strings.TrimPrefix(m.MatchValue, "type:")
		switch m.MatchType {
		case "textType":
			if cfg != nil {
				if mt := cfg.FindMatchType(id); mt != nil && mt.Kind == "text" {
					return "textType", []string{m.MatchValue}, true
				}
			}
			return "", nil, false
		case "fileExt":
			if cfg != nil {
				if exts, ok := cfg.FileGroupExts(id); ok && len(exts) > 0 {
					return "fileExt", exts, true
				}
			}
			return "", nil, false
		}
		return "", nil, false
	}
	switch m.MatchType {
	case "fileExt":
		v := behaviors.RefValues("fileExt", m.MatchValue)
		return "fileExt", v, len(v) > 0
	case "textType":
		v := strings.ToLower(strings.TrimSpace(m.MatchValue))
		if behaviors.IsKnownTextType(v) {
			return "textType", []string{v}, true
		}
		return "", nil, false
	}
	return "", nil, false
}

// MatchSelectedAction 按 mappings 顺序匹配第一个命中的 mapping, 供模拟测试 API 使用。
// 匹配语义与旧 ActionRule 完全一致 (matchActionRule): fileExt 要求选中为文件,
// textType 要求选中为文本; type: 引用由 matchActionRule 内部解析 (cfg 为注册表, 可空)。
func MatchSelectedAction(sa *SelectedAction, isFile bool, content string, cfg *Config) *SelectedMapping {
	if sa == nil {
		return nil
	}
	for i := range sa.Mappings {
		m := &sa.Mappings[i]
		if matchActionRule(cfg, &ActionRule{MatchType: m.MatchType, MatchValue: m.MatchValue}, isFile, content) {
			return m
		}
	}
	return nil
}
