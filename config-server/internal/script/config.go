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
