package script

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"settings/internal/script/generators"
	"settings/internal/script/model"
)

// 类型别名: 数据模型已迁移到 model 包 (阶段 3 拆分),
// 别名保持既有调用方 (main.go handler 等) 无需改动。
type (
	Config            = model.Config
	Keymap            = model.Keymap
	Action            = model.Action
	SelectedAction    = model.SelectedAction
	SelectedMapping   = model.SelectedMapping
	SelectedEntry     = model.SelectedEntry
	ActionScheme      = model.ActionScheme
	ActionRule        = model.ActionRule
	FileGroup         = model.FileGroup
	RuleOptions       = model.RuleOptions
	Options           = model.Options
	WindowGroup       = model.WindowGroup
	Mouse             = model.Mouse
	Scroll            = model.Scroll
	PathVariable      = model.PathVariable
	CommandInputSkin  = model.CommandInputSkin
	QuickSwitchOption = model.QuickSwitchOption
	PluginsOption     = model.PluginsOption
)

var KeyfluxVersion string

// TemplateFuncMap 模板函数表已迁移到 generators 包, 别名保持既有调用方无需改动。
var TemplateFuncMap = generators.TemplateFuncMap

func ParseConfig(file string) (*Config, error) {
	data, err := os.ReadFile(file)
	if err != nil {
		return nil, fmt.Errorf("cannot read file %s: %v", file, err)
	}

	var config Config
	err = json.Unmarshal(data, &config)
	if err != nil {
		return nil, fmt.Errorf("cannot parse config: %v", err)
	}

	config.Options.KeyfluxVersion = KeyfluxVersion
	if config.Options.Mouse.TipSymbol == "" {
		config.Options.Mouse.TipSymbol = "🐶"
	}
	// 皮肤字段全空 (旧配置缺失该段) 时整体填充默认值; 单字段为空则由模板 else 兜底,
	// 两机制互补。默认值真源见 DefaultCommandInputSkin。
	if config.Options.CommandInputSkin == (CommandInputSkin{}) {
		config.Options.CommandInputSkin = DefaultCommandInputSkin()
	}
	// QuickSwitch 段整段为零 (旧配置缺失该段 -> 反序列化为全零) 时整体填充默认值。
	// 边界: 只做「整段为零才补齐」, 绝不逐字段补齐 —— 用户合法地把 autoJumpSave 设为 false
	// (其余字段为默认) 必须原样保留, 故以「全零签名」而非「单字段空值」判定 (同 C# 侧口径)。
	// 默认值真源见 DefaultQuickSwitchOption (三端一致, 有单测守护)。
	if isQuickSwitchZero(config.Options.QuickSwitch) {
		config.Options.QuickSwitch = DefaultQuickSwitchOption()
	}
	// 存量迁移: 旧 actionSchemes → selectedAction 单键分发 (读时一次性, 硬切不回写;
	// 迁移后 ActionSchemes 置 nil, save 序列化不再输出旧段)
	MigrateSelectedAction(&config)

	return &config, nil
}

// DefaultCommandInputSkin 返回命令输入窗口皮肤的全部 18 个字段默认值。
// 字面量必须与 templates/CommandInputSkin.tmpl 头部 else 兜底保持一致, 有单测守护:
// internal/script/skin_defaults_test.go 逐字段比对两处, 不一致即 fail。
func DefaultCommandInputSkin() CommandInputSkin {
	return CommandInputSkin{
		BackgroundColor:       "#FFFFFF",
		BackgroundOpacity:     "0.9",
		BorderWidth:           "3",
		BorderColor:           "#FFFFFF",
		BorderOpacity:         "1.0",
		BorderRadius:          "10",
		CornerColor:           "#000000",
		CornerOpacity:         "0.0",
		GridlineColor:         "#2843AD",
		GridlineOpacity:       "0.04",
		KeyColor:              "#000000",
		KeyOpacity:            "1.0",
		HideAnimationDuration: "0.34",
		WindowYPos:            "0.25",
		WindowWidth:           "700",
		WindowShadowColor:     "#000000",
		WindowShadowOpacity:   "0.5",
		WindowShadowSize:      "3.0",
	}
}

// DefaultQuickSwitchOption 返回「快速切换 QuickSwitch」配置段的出厂默认值
// (设计 §3.1 / PRD §8 裁决)。字面量必须与 AHK 侧 bin/lib/quickswitch/QuickSwitch.ahk 的
// QuickSwitchDefaultConfig() 以及 C# 侧 Models/ConfigReadDefaults.cs 的 QuickSwitchDefaults()
// 逐字段一致, 有单测守护: internal/script/quickswitch_defaults_test.go 逐字段比对 Go 与 AHK 字面量,
// 不一致即 fail, 防止默认值在三端之间漂移。
func DefaultQuickSwitchOption() QuickSwitchOption {
	return QuickSwitchOption{
		CollectEnabled:     true,
		AutoShow:           true,
		AutoJumpOpen:       true,
		AutoJumpSave:       false,
		PollIntervalMs:     800,
		MaxHistory:         200,
		OverlayRows:        8,
		OverlayRowsCompact: 4,
		ExcludedPrefixes:   []string{},
	}
}

// isQuickSwitchZero 判定 quickSwitch 是否为「旧配置缺失该段」的全零签名。
// 因 QuickSwitchOption 含切片字段 (不可用 == 比较结构体), 故逐字段判定;
// 口径与 C# ConfigReadDefaults.IsQuickSwitchUnset 完全一致, 保证引擎与 UI 判定同步。
func isQuickSwitchZero(q QuickSwitchOption) bool {
	return !q.CollectEnabled && !q.AutoShow && !q.AutoJumpOpen && !q.AutoJumpSave &&
		q.PollIntervalMs == 0 && q.MaxHistory == 0 &&
		q.OverlayRows == 0 && q.OverlayRowsCompact == 0 &&
		len(q.ExcludedPrefixes) == 0
}

func SaveConfigFile(config *Config) {
	// 先写到缓冲区,  如果直接写文件的话, 当编码过程遇到错误时, 会导致文件损坏
	buf := new(bytes.Buffer)
	encoder := json.NewEncoder(buf)
	encoder.SetIndent("", "  ")
	encoder.SetEscapeHTML(false)
	if err := encoder.Encode(config); err != nil {
		panic(err)
	}

	if err := os.WriteFile("../data/config.json", buf.Bytes(), 0644); err != nil {
		panic(err)
	}
}
