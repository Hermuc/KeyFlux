// Package behaviors 实现选中动作的「行为包」体系 (docs/CONTRACTS.md §3.9)。
//
// 行为 = 一个自描述目录包: behavior.json (manifest) + 可选脚本 (二期 script entry)。
// 两类来源:
//   - 内置包: 随软件分发, 位于 settings.exe 同级 behaviors/ (仓库 bin/behaviors, 入库);
//   - 用户包: 位于 config.json 同级 behaviors/ (如 ../data/behaviors), 用户经设置界面增删。
//
// 核心不变量:
//   - 规则 (ActionRule.ActionType) 的取值语义 = 行为 ID。内置 11 个基础动作 ID 为保留
//     命名空间且与历史取值完全一致, 因此存量配置零迁移; 渲染期对内置 ID 直通
//     (ResolveRuleAction), 保证 golden 快照 / DumpPlan 对既有配置逐字节稳定。
//   - 保存校验统一为「覆盖检查」: 规则引用的行为必须存在且其 appliesTo 覆盖规则的
//     匹配前提 (取代旧 textTypeActions 静态表, 并补上 fileExt 此前无后端校验的缺口)。
package behaviors

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// SpecVersion 当前行为包格式版本; 与包内 specVersion 字段不一致即拒绝 (前向兼容锚点)。
const SpecVersion = 1

var idPattern = regexp.MustCompile(`^[a-z][a-z0-9_]{0,31}$`)

// pluginActionPattern 插件运行时动作引用 (IAction, 契约 §3.2):
// 行为包 entry.action 可引用 "plugin:<pluginId>:<actionName>" 形态 —— 此类动作不在
// 生成端展开, 由运行时 SelectedAction 分发兜底派发到 ActionRegistry (插件启动期注册)。
var pluginActionPattern = regexp.MustCompile(`^plugin:[a-z][a-z0-9_]{0,31}:[A-Za-z0-9_\-]{1,64}$`)

// BuiltinActionIDs 内置基础动作保留 ID 集 (= AHK ExecuteActionRule 的 case 集 / 前端
// 基础动作词表)。行为包 entry.kind=builtin 的 action 必须取值于此; 用户包 ID 不得占用。
var BuiltinActionIDs = map[string]bool{
	"open_url": true, "open_path": true, "open_folder": true, "magnet_download": true,
	"open": true, "search": true, "run": true,
	"send_keys": true, "script": true, "copy": true,
}

// KnownTextTypes 文本特征词表 (与 script.matchTextType / AHK MatchTextType / C# ActionSchemeCatalog.TextTypes 一致)。
// bilibili = B 站视频号 (AV 号 / BV 号), 2026-09-17 新增。
var KnownTextTypes = map[string]bool{
	"url": true, "path": true, "magnet": true, "bilibili": true, "plain": true,
}

// customRefPrefix 用户自定义匹配类型 (方案 C7) 的引用前缀: 规则的前提值或行为包的 appliesTo
// 写 "type:<id>" 即指向 config.json 的 matchTypes[] / fileGroups[] 定义。本包只需知道
// "是不是引用", 不需要类型定义本身 —— 故不 import model 包 (避免循环依赖); 存在性由调用方
// 经 knownText 钩子裁决。`:` 是 Windows 文件名非法字符, 故该前缀不可能与任何真实文件后缀或
// 内置文本特征值碰撞。
const customRefPrefix = "type:"

// IsCustomRef 判断匹配值是否为自定义类型引用 ("type:<id>")。与 script.isCustomRef 同义
// (纯前缀判断各一份, 无循环依赖)。
func IsCustomRef(v string) bool {
	return strings.HasPrefix(v, customRefPrefix)
}

// AppliesToEntry 生效前提: 行为声明它适用于哪些匹配前提。
// fileExt 用显式后缀集 (["*"]=任意文件), 不存分组名 —— 分组只是设置界面的快捷填入模板,
// 存分组名会在分组改名/删改后断关联 (fileGroups 保存写回任务的同款结论)。
type AppliesToEntry struct {
	Type    string   `json:"type"`            // "fileExt" | "textType"
	Exts    []string `json:"exts,omitempty"`  // fileExt: 覆盖的后缀集 (不含点), "*"=任意文件
	Value   string   `json:"value,omitempty"` // textType: url / path / magnet / bilibili / plain
	Default bool     `json:"default,omitempty"`
}

// EntryParams builtin entry 的默认模板; 规则的 ActionValue/WorkingDir 非空时覆盖之。
type EntryParams struct {
	ActionValue string `json:"actionValue,omitempty"`
	WorkingDir  string `json:"workingDir,omitempty"`
}

// Entry 行为实现入口。一期仅 builtin (生成期展开为基础动作); script (编译期 Include +
// BehaviorRegistry, ctx 对齐 CONTRACTS §3.2 ActionContext) 为二期, 格式先预留。
type Entry struct {
	Kind   string      `json:"kind"`             // "builtin" | "script"
	Action string      `json:"action,omitempty"` // builtin: 基础动作 ID (∈ BuiltinActionIDs)
	Params EntryParams `json:"params,omitempty"` // builtin: 默认模板
	File   string      `json:"file,omitempty"`   // script: 脚本文件名 (二期)
	Func   string      `json:"func,omitempty"`   // script: 入口函数名 (二期)
}

// Pack 行为包 manifest。Source 为加载期附加字段 (builtin/user), 不属于包文件本身。
type Pack struct {
	ID          string           `json:"id"`
	Name        string           `json:"name"`
	NameEn      string           `json:"nameEn,omitempty"`
	Version     string           `json:"version,omitempty"`
	Description string           `json:"description,omitempty"`
	SpecVersion int              `json:"specVersion"`
	AppliesTo   []AppliesToEntry `json:"appliesTo"`
	Entry       Entry            `json:"entry"`
	Permissions []string         `json:"permissions,omitempty"`
	// BoundTypeID 该包"强绑定"的自定义匹配类型 id (方案 C7 路径①: 建类型时同时建专属行为)。
	// 仅供 UI 展示"该类型绑定的行为"(P1-5), 不参与覆盖判定 —— 覆盖仍由 appliesTo 唯一裁决,
	// 否则会出现"绑定即覆盖"的隐式语义, 与 Catalog.Covers 的单向推导冲突。
	BoundTypeID string `json:"boundTypeId,omitempty"`
	Source      string `json:"source,omitempty"`
}

// Catalog 行为目录: 内置包在前、用户包在后, 各自按 ID 字典序 (决定 DefaultFor 的稳定序)。
// 加载容错: 目录缺失 = 空; 单个坏包跳过并记入 Errors (错误隔离, 不拖垮整个目录)。
type Catalog struct {
	Packs  []*Pack
	Errors []string
}

func (c *Catalog) Get(id string) *Pack {
	for _, p := range c.Packs {
		if p.ID == id {
			return p
		}
	}
	return nil
}

// loadDir 读取一个包目录; 返回解析成功的包与错误累积。
func loadDir(dir, source string) ([]*Pack, []string, error) {
	var packs []*Pack
	var errs []string
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil, nil // 目录缺失 = 无该来源包 (正常场景)
		}
		return nil, nil, err
	}
	for _, de := range entries {
		if !de.IsDir() {
			continue
		}
		p, err := readPack(filepath.Join(dir, de.Name()))
		if err != nil {
			errs = append(errs, fmt.Sprintf("%s: %v", de.Name(), err))
			continue
		}
		p.Source = source
		packs = append(packs, p)
	}
	return packs, errs, nil
}

func readPack(dir string) (*Pack, error) {
	raw, err := os.ReadFile(filepath.Join(dir, "behavior.json"))
	if err != nil {
		return nil, err
	}
	raw = bytes.TrimPrefix(raw, []byte{0xEF, 0xBB, 0xBF}) // 防误存 BOM (Go json 不容忍)
	var p Pack
	if err := json.Unmarshal(raw, &p); err != nil {
		return nil, fmt.Errorf("behavior.json 解析失败: %w", err)
	}
	if p.ID != filepath.Base(dir) {
		return nil, fmt.Errorf("目录名 %q 与 manifest id %q 不一致", filepath.Base(dir), p.ID)
	}
	if err := ValidateManifest(&p); err != nil {
		return nil, err
	}
	return &p, nil
}

// LoadCatalog 加载内置包目录、用户包目录与额外来源目录, 构建稳定排序的目录。
// extraDirs = 插件贡献的行为包目录 (plugins/<id>/behaviors, §3.9 同格式),
// 排在最后: 同 ID 冲突时 Catalog.Get 先到者胜 (优先级 builtin > user > 插件贡献)。
func LoadCatalog(builtinDir, userDir string, extraDirs ...string) *Catalog {
	c := &Catalog{}
	builtin, errs, err := loadDir(builtinDir, "builtin")
	if err != nil {
		c.Errors = append(c.Errors, fmt.Sprintf("builtin: %v", err))
	}
	c.Errors = append(c.Errors, errs...)
	user, errs, err := loadDir(userDir, "user")
	if err != nil {
		c.Errors = append(c.Errors, fmt.Sprintf("user: %v", err))
	}
	c.Errors = append(c.Errors, errs...)
	sortPacks(builtin)
	sortPacks(user)
	c.Packs = append(c.Packs, builtin...)
	c.Packs = append(c.Packs, user...)
	for i, dir := range extraDirs {
		packs, errs, err := loadDir(dir, fmt.Sprintf("plugin%d", i+1))
		if err != nil {
			c.Errors = append(c.Errors, fmt.Sprintf("plugin%d: %v", i+1, err))
		}
		c.Errors = append(c.Errors, errs...)
		sortPacks(packs)
		// 同 ID 先到者胜 (builtin > user > 插件贡献): 冲突包跳过并注记,
		// 避免行为库 UI 出现同 ID 重复条目
		for _, p := range packs {
			if c.Get(p.ID) != nil {
				c.Errors = append(c.Errors, fmt.Sprintf("plugin%d: 贡献包 ID %q 与既有行为冲突, 已跳过", i+1, p.ID))
				continue
			}
			c.Packs = append(c.Packs, p)
		}
	}
	return c
}

func sortPacks(packs []*Pack) {
	sort.SliceStable(packs, func(i, j int) bool { return packs[i].ID < packs[j].ID })
}

// --------------------------------------------------------------- 覆盖语义

// normalizeExt 归一化单个后缀: 去空白、去两端点 (与前端 NormalizeExts 语义一致)。
func normalizeExt(v string) string {
	return strings.Trim(strings.TrimSpace(v), ".")
}

// RefValues 把规则的匹配前提展开为值集 (供保存校验与删除校验共用)。
//
// 已知限制 (方案 C7 §D.3.5 / R3-8): 自定义类型引用 "type:<id>" 在本函数里退化为
// **整串 token 比较** —— fileExt 侧不会展开为该分组的后缀集, 故 ValidateDelete 的
// "值级覆盖检查" 对引用成立的映射不生效 (删除文件类类型时不会拦"空前提桶")。
// 引用侧的引用计数检查 (RuleRef.ActionType) 不受影响。彻底修正见 P2 项
// "RefValues 类型感知"。
func RefValues(matchType, matchValue string) []string {
	if matchType == "fileExt" {
		if strings.TrimSpace(matchValue) == "*" {
			return []string{"*"}
		}
		var out []string
		for _, v := range strings.Split(matchValue, ",") {
			if v = normalizeExt(v); v != "" {
				out = append(out, v)
			}
		}
		return out
	}
	if v := strings.TrimSpace(matchValue); v != "" {
		return []string{v}
	}
	return nil
}

func entryValues(e AppliesToEntry) []string {
	if e.Type == "fileExt" {
		return e.Exts
	}
	return []string{e.Value}
}

// entryCovers 判断单条前提是否覆盖规则值集: fileExt 要求规则值集 ⊆ 前提值集
// ("*" 覆盖任意); textType 要求特征值相等, 但自定义类型引用 (type:<id>) 例外 ——
// 见下方"两段式覆盖"。
func entryCovers(e AppliesToEntry, matchType string, values []string) bool {
	if e.Type != matchType {
		return false
	}
	switch e.Type {
	case "fileExt":
		packWildcard := containsFold(e.Exts, "*")
		for _, v := range values {
			if v == "*" {
				if !packWildcard {
					return false // 任意文件规则只能依赖通配前提
				}
				continue
			}
			if !packWildcard && !containsFold(e.Exts, v) {
				return false
			}
		}
		return true
	case "textType":
		if len(values) != 1 {
			return false
		}
		if strings.EqualFold(e.Value, values[0]) {
			return true
		}
		// 两段式覆盖 (方案 C7 "P0-6 留桩不死胡同"的结构性解法):
		// 自定义文本类型的判定是 url/path/magnet/plain 之外的附加特征, 通用文本行为对它同样
		// 适用 —— 故 "plain" 前提视为覆盖任意自定义类型, 使 5 个内置 plain 包
		// (copy / run / script / search / send_keys) 自动成为每个自定义类型的"继承行为"。
		// 效果: 用户"先留桩、后补专属行为"时, 添加映射弹窗的勾选列表恒非空、
		// AddMappingVm.CanConfirm 恒可用, 不会出现"空列表 + 禁用按钮"的死胡同;
		// 专属包 (appliesTo 写 type:<id>) 仍按上面的值相等分支优先命中。
		return IsCustomRef(values[0]) && strings.EqualFold(e.Value, "plain")
	}
	return false
}

func containsFold(list []string, v string) bool {
	for _, item := range list {
		if strings.EqualFold(normalizeExt(item), normalizeExt(v)) {
			return true
		}
	}
	return false
}

// Covers 判断行为 id 是否覆盖给定匹配前提 (值集语义见 entryCovers)。
func (c *Catalog) Covers(id, matchType string, values []string) bool {
	p := c.Get(id)
	if p == nil {
		return false
	}
	for _, e := range p.AppliesTo {
		if entryCovers(e, matchType, values) {
			return true
		}
	}
	return false
}

// DefaultFor 返回该前提桶的默认行为 ID: 取目录序中第一条带 default 标记且覆盖前提的包;
// 无标记时回退第一条覆盖包 (保持旧行为联动「切换特征自动落默认」的可用性)。
func (c *Catalog) DefaultFor(matchType string, values []string) string {
	var firstCovered string
	for _, p := range c.Packs {
		for _, e := range p.AppliesTo {
			if e.Type != matchType || !entryCovers(e, matchType, values) {
				continue
			}
			if e.Default {
				return p.ID
			}
			if firstCovered == "" {
				firstCovered = p.ID
			}
			break
		}
	}
	return firstCovered
}

// --------------------------------------------------------------- 渲染期展开

// ResolveRuleAction 把规则引用的行为 ID 解析为渲染用的 (actionType, actionValue, workingDir)。
// 内置基础动作 ID 直通 —— 存量配置的渲染产物与 golden 快照/DumpPlan 逐字节不变;
// 用户包 builtin entry 用包默认模板补空值; 包缺失或 script entry (二期) 原样返回,
// 由保存校验 (引用存在性) 与生成警告兜底。
func ResolveRuleAction(c *Catalog, actionType, actionValue, workingDir string) (string, string, string) {
	if c == nil || BuiltinActionIDs[actionType] {
		return actionType, actionValue, workingDir
	}
	p := c.Get(actionType)
	if p == nil || p.Entry.Kind != "builtin" {
		return actionType, actionValue, workingDir
	}
	if actionValue == "" {
		actionValue = p.Entry.Params.ActionValue
	}
	if workingDir == "" {
		workingDir = p.Entry.Params.WorkingDir
	}
	return p.Entry.Action, actionValue, workingDir
}

// --------------------------------------------------------------- 校验

// ValidateManifest 校验包 manifest 结构合法性 (加载期与创建/更新 API 共用)。
//
// knownText 为可选的"自定义匹配类型存在性"钩子 (方案 C7 §D.3.5): 保存链路
// (WriteUserPack) 传入真实解析器, 于是 appliesTo/boundTypeId 里引用不存在的类型会被拒绝;
// 加载链路 (readPack) 不传 —— 与 script.ValidateSelectedAction 的容忍口径一致, 不把含
// 遗留数据的旧配置/旧行为包锁死在读路径上 (未定义的 type: 引用运行时不命中, 生成端会跳过)。
func ValidateManifest(p *Pack, knownText ...func(string) bool) error {
	var resolve func(string) bool
	if len(knownText) > 0 {
		resolve = knownText[0]
	}
	if !idPattern.MatchString(p.ID) {
		return fmt.Errorf("行为 ID %q 不合法 (须匹配 ^[a-z][a-z0-9_]{0,31}$)", p.ID)
	}
	if p.SpecVersion != SpecVersion {
		return fmt.Errorf("specVersion 必须为 %d (当前 %d)", SpecVersion, p.SpecVersion)
	}
	if strings.TrimSpace(p.Name) == "" {
		return fmt.Errorf("行为「%s」缺少名称 (name)", p.ID)
	}
	if len(p.AppliesTo) == 0 {
		return fmt.Errorf("行为「%s」缺少生效前提 (appliesTo)", p.ID)
	}
	if IsCustomRef(strings.TrimSpace(p.BoundTypeID)) {
		// 容错: 允许写 "type:<id>" 或裸 id 两种形态, 归一后校验存在性
		p.BoundTypeID = strings.TrimSpace(strings.TrimPrefix(strings.TrimSpace(p.BoundTypeID), customRefPrefix))
	}
	if p.BoundTypeID != "" && resolve != nil && !resolve(p.BoundTypeID) {
		return fmt.Errorf("行为「%s」绑定的匹配类型「%s」不存在", p.ID, p.BoundTypeID)
	}
	for i := range p.AppliesTo {
		e := &p.AppliesTo[i]
		switch e.Type {
		case "fileExt":
			if len(e.Exts) == 0 {
				return fmt.Errorf("行为「%s」第 %d 条匹配条件缺少文件扩展名", p.ID, i+1)
			}
			for j, ext := range e.Exts {
				if normalizeExt(ext) == "" {
					return fmt.Errorf("行为「%s」第 %d 条匹配条件的第 %d 个扩展名为空", p.ID, i+1, j+1)
				}
			}
		case "textType":
			v := strings.ToLower(strings.TrimSpace(e.Value))
			if IsCustomRef(v) {
				// 自定义匹配类型引用 (方案 C7): 存在性只在保存链路裁决 (见函数头注释)
				if resolve != nil && !resolve(strings.TrimPrefix(v, customRefPrefix)) {
					return fmt.Errorf("行为「%s」第 %d 条匹配条件引用的匹配类型「%s」不存在", p.ID, i+1, e.Value)
				}
				e.Value = v
				continue
			}
			if !KnownTextTypes[v] {
				return fmt.Errorf("行为「%s」第 %d 条匹配条件的文本特征「%s」无效（可选：链接 / 路径 / 磁力链接 / 纯文本，或已自定义的匹配类型）", p.ID, i+1, e.Value)
			}
			e.Value = v
		default:
			return fmt.Errorf("行为「%s」第 %d 条匹配条件的分类「%s」无效（可选：文本内容 / 文件类型）", p.ID, i+1, e.Type)
		}
	}
	switch p.Entry.Kind {
	case "builtin":
		// 内置基础动作白名单, 或插件运行时动作引用 (plugin:<id>:<name>, 运行时经
		// ActionRegistry 兜底派发; 插件未注册时运行时静默忽略并提示)
		if !BuiltinActionIDs[p.Entry.Action] && !pluginActionPattern.MatchString(p.Entry.Action) {
			return fmt.Errorf("行为「%s」的 entry.action %q 不是内置基础动作或插件动作引用", p.ID, p.Entry.Action)
		}
	case "script":
		if strings.TrimSpace(p.Entry.File) == "" || strings.TrimSpace(p.Entry.Func) == "" {
			return fmt.Errorf("行为「%s」的 script entry 缺少 file 或 func", p.ID)
		}
	default:
		return fmt.Errorf("行为「%s」的 entry.kind %q 不合法 (可选: builtin / script)", p.ID, p.Entry.Kind)
	}
	return nil
}

// RuleRef 保存校验/删除校验所需的规则投影 (避免反向依赖 script/model 包)。
type RuleRef struct {
	MatchType  string
	MatchValue string
	ActionType string
}

// ValidateDelete 校验删除行为包是否安全:
//  1. 内置包不可删除;
//  2. 被任何规则 actionType 引用 → 拒绝 (引用会悬空);
//  3. 值级覆盖检查: 删除后, 若该包前提覆盖的某个值仍被引用 (其他规则或其他行为前提)
//     且再无任何启用行为覆盖 → 拒绝 (防止「图片后缀没有任何可编辑行为」的空前提桶)。
func ValidateDelete(c *Catalog, id string, refs []RuleRef) error {
	p := c.Get(id)
	if p == nil {
		return fmt.Errorf("行为「%s」不存在", id)
	}
	if p.Source != "user" {
		return fmt.Errorf("内置行为「%s」不可删除", p.Name)
	}
	refCount := 0
	referenced := map[[2]string]bool{}
	for _, r := range refs {
		if r.ActionType == id {
			refCount++
		}
		for _, v := range RefValues(r.MatchType, r.MatchValue) {
			referenced[[2]string{r.MatchType, v}] = true
		}
	}
	if refCount > 0 {
		return fmt.Errorf("行为「%s」仍被 %d 条映射引用，请先修改或删除相应映射", p.Name, refCount)
	}
	var others []*Pack
	for _, o := range c.Packs {
		if o.ID != id {
			others = append(others, o)
		}
	}
	for _, o := range others {
		for _, e := range o.AppliesTo {
			for _, v := range entryValues(e) {
				referenced[[2]string{e.Type, v}] = true
			}
		}
	}
	var uncovered []string
	for _, e := range p.AppliesTo {
		for _, v := range entryValues(e) {
			if !referenced[[2]string{e.Type, v}] {
				continue // 无任何引用的前提值随包一并消失, 不构成空桶
			}
			if !coveredBy(others, e.Type, v) {
				uncovered = append(uncovered, displayValue(e.Type, v))
			}
		}
	}
	if len(uncovered) > 0 {
		return fmt.Errorf("删除「%s」后以下匹配条件将没有可用行为：%s", p.Name, strings.Join(uncovered, "、"))
	}
	return nil
}

func coveredBy(packs []*Pack, matchType string, v string) bool {
	for _, p := range packs {
		for _, e := range p.AppliesTo {
			if entryCovers(e, matchType, []string{v}) {
				return true
			}
		}
	}
	return false
}

func displayValue(matchType, v string) string {
	if matchType == "textType" {
		return "文本特征 " + v
	}
	if v == "*" {
		return "任意文件"
	}
	return "后缀 ." + strings.TrimPrefix(v, ".")
}

// WriteUserPack 校验并把用户包写入磁盘 (dirName = id, manifest 缩进 JSON, 无 BOM)。
// knownText 为可选的自定义匹配类型存在性钩子 (见 ValidateManifest), 保存链路应传入。
func WriteUserPack(userDir string, p *Pack, knownText ...func(string) bool) error {
	if p.Source == "builtin" || BuiltinActionIDs[p.ID] {
		return fmt.Errorf("行为标识「%s」与内置行为冲突", p.ID)
	}
	if err := ValidateManifest(p, knownText...); err != nil {
		return err
	}
	raw, err := json.MarshalIndent(p, "", "  ")
	if err != nil {
		return err
	}
	dir := filepath.Join(userDir, p.ID)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(dir, "behavior.json"), append(raw, '\n'), 0o644)
}

// RemoveUserPack 删除用户包目录 (id 已由正则保证无路径穿越字符)。
func RemoveUserPack(userDir, id string) error {
	if BuiltinActionIDs[id] {
		return fmt.Errorf("行为标识「%s」与内置行为冲突", id)
	}
	return os.RemoveAll(filepath.Join(userDir, id))
}
