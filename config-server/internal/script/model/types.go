// Package model 承载 KeyFlux 配置的数据模型与纯配置方法。
// 依赖方向: generators -> model <- script, 避免生成逻辑与数据模型循环依赖。
package model

// RemapKey 是"重映射按键"动作的 TypeID, 生成端与渲染端共用
const RemapKey = 5

type Config struct {
	Keymaps       []Keymap       `json:"keymaps,omitempty"`
	Options       Options        `json:"options,omitempty"`
	SelectedAction *SelectedAction `json:"selectedAction,omitempty"` // 选中动作单键分发 (方案 D); 迁移后由 ParseConfig 保证非 nil, save 恒输出
	ActionSchemes []ActionScheme `json:"actionSchemes,omitempty"`  // 旧多方案结构: 仅迁移读取用, ParseConfig 迁移后置 nil, save 不再输出
	FileGroups    []FileGroup    `json:"fileGroups,omitempty"` // 文件分组: 前端「文件后缀」条件值的快捷填充数据, 非独立匹配类型
	OverviewDocMd string         `json:"overviewDocMd,omitempty"` // 自定义总览页 Markdown, 设置界面优先展示; 为空时展示默认 config_doc.md
	KeyMapping    string         `json:"-"`
}

type Keymap struct {
	ID        int                 `json:"id"`
	Name      string              `json:"name"`
	Enable    bool                `json:"enable"`
	Hotkey    string              `json:"hotkey"`
	ParentID  int                 `json:"parentID"`
	Delay     int                 `json:"delay"`
	DisableAt string              `json:"disableAt"`
	Hotkeys   map[string][]Action `json:"hotkeys"`
}

// 选中动作单键分发 (方案 D): 选中内容后按单一热键触发, 按 mappings 顺序匹配第一个命中的
// mapping, 弹出其 entries 菜单 (最多 9 项) 由用户按数字键选择行为。
// 旧多方案 ActionScheme 结构于 2026-09 重构为单键分发, 存量配置由 ParseConfig 读时一次性迁移。
type SelectedAction struct {
	Hotkey   string            `json:"hotkey"`
	Enable   bool              `json:"enable"`
	Mappings []SelectedMapping `json:"mappings"` // 有序 = 匹配优先级
}

// SelectedMapping 一个匹配前提桶: 同 matchType+matchValue 的行为菜单。
// 语义对齐旧 ActionRule 的 matchType/matchValue (fileExt=逗号分隔后缀, textType=url/path/magnet/plain)。
type SelectedMapping struct {
	MatchType  string          `json:"matchType"` // "fileExt" | "textType"
	MatchValue string          `json:"matchValue"`
	Entries    []SelectedEntry `json:"entries"` // 1..9 项, 顺序即菜单序号
}

// SelectedEntry 菜单项: behavior = 行为库 ID (内置 11 个基础动作 ID 或用户行为包 ID)。
// actionValue/workingDir 非空时覆盖行为包默认模板, 空则用包默认 (复用 behaviors.ResolveRuleAction 语义)。
// options 沿用旧 rule 的三开关 (确认/复制/清空), 渲染数据数组暂不消费, 语义由 AHK 端 (任务 #30) 定义。
type SelectedEntry struct {
	Behavior    string      `json:"behavior"`
	ActionValue string      `json:"actionValue,omitempty"`
	WorkingDir  string      `json:"workingDir,omitempty"`
	Options     RuleOptions `json:"options"`
}

// 选中动作方案 (旧多方案结构, 已于 2026-09 重构为单键分发 SelectedAction):
// 仅保留供 ParseConfig 存量迁移读取, 不再渲染与校验。
// 变量约定: 在命令或脚本中使用 %selected% 表示当前选中的文本或文件路径(多文件用换行分隔)
type ActionScheme struct {
	ID     int          `json:"id"`
	Name   string       `json:"name"`
	Hotkey string       `json:"hotkey"`
	Enable bool         `json:"enable"`
	Rules  []ActionRule `json:"rules"`

	// 传输层字段 (不落盘): 保存/新建方案后重启 KeyFlux 失败时由 handler 置 true 随响应返回;
	// omitempty 保证 false 时不序列化, config.json 落盘与导入导出均不受影响
	RestartFailed bool `json:"restartFailed,omitempty"`
}

// 文件分组: 供前端「文件后缀」条件值快捷填充 (选择分组 -> 展开为逗号分隔后缀列表), 非独立匹配类型
// 引擎层不感知分组概念, 规则 matchValue 始终是后缀列表; 分组定义是配置数据, 用户可自行增改
// (2026-08-24 由 fileGroup 匹配类型收敛而来: 原 Go/AHK 双端内置表已删除)
type FileGroup struct {
	Name  string   `json:"name"`  // 分组标识 (英文, 如 image)
	Label string   `json:"label"` // 中文显示名 (如 图片)
	Exts  []string `json:"exts"`  // 后缀列表 (不含点, 如 ["jpg","jpeg"])
}

// 规则按 Priority 升序匹配, 第一个匹配的规则生效
// MatchType: fileExt(文件后缀) / textType(文本特征) (default(兜底) 已于 2026-09 移除, 存量不再命中)
//   - fileExt 条件值为逗号分隔后缀列表, 可用配置中的 fileGroups 快捷填充
// ActionType: open(程序打开) / search(搜索) / run(执行命令) / send_keys(发送按键) / script(AHK脚本) / copy(复制到剪贴板)
//   + textType 专用行为: open_url(默认浏览器打开网址) / open_path(系统关联打开) / open_folder(打开文件夹)
//     / magnet_download(磁力链接下载) / open_registry(注册表定位); 特征与行为的合法组合见 actionscheme.go 的 textTypeActions
// 注: textType 特征的 actionValue 为「特征选择」而非条件值, 专用行为不接受命令模板 (actionValue 留空)
type ActionRule struct {
	Priority    int         `json:"priority"`
	MatchType   string      `json:"matchType"`
	MatchValue  string      `json:"matchValue"`
	ActionType  string      `json:"actionType"`
	ActionValue string      `json:"actionValue"`
	WorkingDir  string      `json:"workingDir,omitempty"`
	Options     RuleOptions `json:"options"`
}

type RuleOptions struct {
	CopyToClipboard bool `json:"copyToClipboard"` // 动作执行后把选中内容保留到剪贴板 (便于执行完成后直接粘贴)
	ClearSelection  bool `json:"clearSelection"`  // 执行后清空选中
	Confirm         bool `json:"confirm"`         // 执行前显示确认提示
}

type Action struct {
	WindowGroupID int    `json:"windowGroupID"`
	TypeID        int    `json:"actionTypeID"`
	Comment       string `json:"comment,omitempty"`
	Hotkey        string `json:"hotkey,omitempty"`
	// 下面的字段因动作类型而异
	KeysToSend         string `json:"keysToSend,omitempty"`
	RemapToKey         string `json:"remapToKey,omitempty"`
	ValueID            int    `json:"actionValueID,omitempty"`
	WinTitle           string `json:"winTitle,omitempty"`
	Target             string `json:"target,omitempty"`
	Args               string `json:"args,omitempty"`
	WorkingDir         string `json:"workingDir,omitempty"`
	RunAsAdmin         bool   `json:"runAsAdmin,omitempty"`
	RunInBackground    bool   `json:"runInBackground,omitempty"`
	DetectHiddenWindow bool   `json:"detectHiddenWindow,omitempty"`
	AHKCode            string `json:"ahkCode,omitempty"`

	RemapInHotIf bool `json:"-"`
}

type Options struct {
	HideMatrix       bool             `json:"hideMatrix"`
	KeyfluxVersion  string           `json:"keyfluxVersion"`
	WindowGroups     []WindowGroup    `json:"windowGroups"`
	Mouse            Mouse            `json:"mouse"`
	Scroll           Scroll           `json:"scroll"`
	CommandInputSkin CommandInputSkin `json:"commandInputSkin"`
	PathVariables    []PathVariable   `json:"pathVariables"`
	Startup          bool             `json:"startup"`
	Language         string           `json:"language"`
	KeyMapping       string           `json:"keyMapping"`
	KeyboardLayout   string           `json:"keyboardLayout"`
}

type WindowGroup struct {
	ID            int    `json:"id"`
	Name          string `json:"name"`
	Value         string `json:"value,omitempty"`
	ConditionType int    `json:"conditionType,omitempty"`
}

type Mouse struct {
	KeepMouseMode bool   `json:"keepMouseMode"`
	ShowTip       bool   `json:"showTip"`
	TipSymbol     string `json:"tipSymbol"`
	Delay1        string `json:"delay1"`
	Delay2        string `json:"delay2"`
	FastSingle    string `json:"fastSingle"`
	FastRepeat    string `json:"fastRepeat"`
	SlowSingle    string `json:"slowSingle"`
	SlowRepeat    string `json:"slowRepeat"`
}

type Scroll struct {
	Delay1        string `json:"delay1"`
	Delay2        string `json:"delay2"`
	OnceLineCount string `json:"onceLineCount"`
}

type PathVariable struct {
	Name  string `json:"name"`
	Value string `json:"value"`
}

type CommandInputSkin struct {
	BackgroundColor       string `json:"backgroundColor"`
	BackgroundOpacity     string `json:"backgroundOpacity"`
	BorderWidth           string `json:"borderWidth"`
	BorderColor           string `json:"borderColor"`
	BorderOpacity         string `json:"borderOpacity"`
	BorderRadius          string `json:"borderRadius"`
	CornerColor           string `json:"cornerColor"`
	CornerOpacity         string `json:"cornerOpacity"`
	GridlineColor         string `json:"gridlineColor"`
	GridlineOpacity       string `json:"gridlineOpacity"`
	KeyColor              string `json:"keyColor"`
	KeyOpacity            string `json:"keyOpacity"`
	HideAnimationDuration string `json:"hideAnimationDuration"`
	WindowYPos            string `json:"windowYPos"`
	WindowWidth           string `json:"windowWidth"`
	WindowShadowColor     string `json:"windowShadowColor"`
	WindowShadowOpacity   string `json:"windowShadowOpacity"`
	WindowShadowSize      string `json:"windowShadowSize"`
}
