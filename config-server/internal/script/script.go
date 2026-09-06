package script

import (
	"os"
	"path/filepath"
	"settings/internal/behaviors"
	"settings/internal/script/generators"
	"strings"
	"text/template"
)

func GenerateScripts(config *Config) {
	// 行为目录: 渲染期据此把规则引用的用户行为 ID 展开为基础动作 (内置 ID 直通);
	// 运行时 cwd=bin, 用户包随配置在 ../data/behaviors
	generators.BehaviorCatalog = LoadBehaviorCatalog("../data/config.json")

	Preprocess(config)

	if err := SaveAHK(config, "./templates/KeyFlux.tmpl", "../bin/KeyFlux.ahk"); err != nil {
		panic(err)
	}
	if err := SaveAHK(config, "./templates/CommandInputSkin.tmpl", "../bin/CommandInputSkin.txt"); err != nil {
		panic(err)
	}
}

// LoadBehaviorCatalog 加载行为目录: 内置包在 settings.exe 同级 behaviors/, 用户包在
// config.json 同级 behaviors/。CLI 与运行时的 cwd 不同, 故以可执行文件与配置文件路径
// 推导目录位置, 不依赖当前工作目录。
func LoadBehaviorCatalog(configPath string) *behaviors.Catalog {
	builtinDir := "behaviors"
	if exePath, err := os.Executable(); err == nil {
		builtinDir = filepath.Join(filepath.Dir(exePath), "behaviors")
	}
	userDir := filepath.Join(filepath.Dir(configPath), "behaviors")
	return behaviors.LoadCatalog(builtinDir, userDir)
}

// Preprocess 对配置做生成前的预处理。
// 添加一个隐藏的全局热键(!f17), 且免疫 suspend, 否则 ahk 的 suspend 会把键盘钩子临时移除。
// 注意: 任何生成验证命令(如 GenerateAHK)都必须调用本函数, 保证验证路径与运行时路径(GenerateScripts)一致。
func Preprocess(cfg *Config) {
	for _, km := range cfg.Keymaps {
		if km.ID == 1 {
			km.Hotkeys["!f17"] = []Action{{TypeID: 9, ValueID: 2}}
			return
		}
	}
}

func SaveAHK(data *Config, templateFile, outputFile string) error {
	generators.Cfg = data
	files := []string{
		templateFile,
	}
	ts, err := template.New(filepath.Base(templateFile)).Funcs(generators.TemplateFuncMap).ParseFiles(files...)
	if err != nil {
		return err
	}

	// 用 Go 代码生成 AHK 脚本时会使用 \n 导致换行符不统一
	// 先输出到一个字符串, 然后对换行符进行统一, 把 \n 改成 \r\n
	builder := new(strings.Builder)
	err = ts.Execute(builder, data)
	if err != nil {
		return err
	}
	res := builder.String()
	res = strings.ReplaceAll(res, "\r\n", "\n")
	res = strings.ReplaceAll(res, "\n", "\r\n")

	f, err := os.Create(outputFile)
	if err != nil {
		return err
	}
	//goland:noinspection GoUnhandledErrorResult
	defer f.Close()

	// 因为模板文件就是 UTF-8 with BOM,  所以输出文件也是 UTF-8 with BOM
	// _, _ = f.Write([]byte{0xef, 0xbb, 0xbf}) // 写入 utf-8 的 BOM (0xefbbbf)
	_, err = f.Write([]byte(res))
	return err
}
