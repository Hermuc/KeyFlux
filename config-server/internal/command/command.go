package command

import (
	"errors"
	"fmt"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"settings/internal/script"
	"settings/internal/script/generators"
)

var Map = map[string]func(args ...string){
	"GenerateAHK":     GenerateAHK,
	"DumpPlan":        DumpPlan,
	"ChangeVersion":   ChangeVersion,
	"GenerateScripts": GenerateScripts,
	"UseOriginalAHK":  UseOriginalAHK,
}

var logger = log.New(os.Stderr, "", 0)

func GenerateAHK(args ...string) {
	if len(os.Args) < 5 {
		logger.Fatal("GenerateAHK requires 3 arguments, for example: GenerateAHK ./config.json ./templates/script.ahk ./output.ahk")
	}
	configFile := os.Args[2]
	templateFile := os.Args[3]
	outputFile := os.Args[4]

	config, err := script.ParseConfig(configFile)
	if err != nil {
		logger.Fatal(err)
	}

	// 与运行时路径(GenerateScripts)保持一致: 先预处理(注入 !f17 免疫热键等)再生成,
	// 否则验证产物与真实运行产物不一致, 无法用于零行为变更验证/Oracle diff
	generators.BehaviorCatalog = script.LoadBehaviorCatalog(configFile)
	// 插件注入目录 (config.json 同级 plugins/), 见 generators/plugins.go
	generators.SetPluginsDir(filepath.Join(filepath.Dir(configFile), "plugins"))
	script.Preprocess(config)

	if err := script.SaveAHK(config, templateFile, outputFile); err != nil {
		logger.Fatal(err)
	}
}

// DumpPlan 输出注册计划 JSON (Oracle 机制的生成端义务, 见 docs/CONTRACTS.md §5)。
// 用法: settings.exe DumpPlan <config.json> <plan.json>
// 未来与 AHK 运行时加载器导出的计划 diff, 验证零行为变更。
func DumpPlan(args ...string) {
	if len(os.Args) < 4 {
		logger.Fatal("DumpPlan requires 2 arguments, for example: DumpPlan ./config.json ./plan.json")
	}
	configFile := os.Args[2]
	outputFile := os.Args[3]

	config, err := script.ParseConfig(configFile)
	if err != nil {
		logger.Fatal(err)
	}

	// 与运行时路径(GenerateScripts)保持一致: 先预处理(注入 !f17 免疫热键等)再推导计划,
	// 否则计划与真实运行注册不一致, Oracle diff 失去意义
	generators.BehaviorCatalog = script.LoadBehaviorCatalog(configFile)
	script.Preprocess(config)

	if err := generators.WritePlan(config, outputFile); err != nil {
		logger.Fatal(err)
	}
}

func ChangeVersion(args ...string) {
	config, err := script.ParseConfig("../data/config.json")
	if err != nil {
		panic(err)
	}
	config.Options.KeyfluxVersion = args[0]
	config.Options.Language = "" // 重置语言
	script.SaveConfigFile(config)
	script.GenerateScripts(config)
}

func GenerateScripts(args ...string) {
	config, err := script.ParseConfig("../data/config.json")
	if err != nil {
		panic(err)
	}
	script.GenerateScripts(config)
}

func UseOriginalAHK(args ...string) {
	defer func() {
		fmt.Println()
	}()

	exe := "bin\\AutoHotkey64.exe"
	if _, err := os.Stat(exe); errors.Is(err, os.ErrNotExist) {
		fmt.Println("Error: file", exe, "does not exist")
		return
	}
	if err := execCmd("cmd.exe", "/c", "copy /y "+exe+" KeyFlux.exe"); err != nil {
		fmt.Println("\nPlease close KeyFlux and retry")
		return
	}
	if err := execCmd("cmd.exe", "/c", "copy /y bin\\Launcher.ahk KeyFlux.ahk"); err != nil {
		panic(err)
	}
	fmt.Println("\ndone!")
}

func execCmd(exe string, args ...string) error {
	cmd := exec.Command(exe, args...)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	if err := cmd.Start(); err != nil {
		return err
	}
	if err := cmd.Wait(); err != nil {
		return err
	}
	return nil
}
