package main

import (
	"encoding/json"
	"fmt"
	"os"

	"settings/internal/command"
	"settings/internal/matrix"
	"settings/internal/proc"
	"settings/internal/server"
)

func main() {
	if len(os.Args) >= 2 {
		if handler, ok := command.Map[os.Args[1]]; ok {
			handler(os.Args[2:]...)
			return
		}
	}

	hasError := make(chan struct{})
	rainDone := make(chan struct{})
	debug := len(os.Args) == 2 && os.Args[1] == "debug"
	// headless 模式: 供 Avalonia 壳以子进程方式拉起, 无代码雨/无浏览器/不开 CORS, 通过 stdout 端口通告行告知实际监听端口
	headless := len(os.Args) == 2 && os.Args[1] == "--headless"

	if !debug {
		if headless || hideMatrix() {
			close(rainDone)
			if !headless {
				fmt.Println("KeyFlux config server is running...")
			}
		} else {
			go matrix.DigitalRain(hasError, rainDone)
		}
	}
	if debug {
		hasError = nil
	}

	// GenerateShortcuts 与预热均不阻塞 listen: 提权 exe 的 spawn 可能同步慢失败 ~2s
	// (实测), 曾把"连接后端"整体拖到 2.1s —— 两者都与 HTTP 服务无依赖, 并行化后
	// 进程启动 ~70ms 即通告端口。
	go proc.ExecCmd("./KeyFlux.exe", "/script", "./bin/MiscTools.ahk", "GenerateShortcuts")
	go server.PreloadStartup() // 异步预热开机自启缓存: 与 GUI 冷启动并行, 首次 GET /config 免等 schtasks
	server.Run(hasError, rainDone, debug, headless)
}

func hideMatrix() bool {
	var config struct {
		Options struct {
			HideMatrix bool `json:"hideMatrix"`
		} `json:"options"`
	}

	data, err := os.ReadFile("../data/config.json")
	if err != nil {
		return false
	}

	err = json.Unmarshal(data, &config)
	if err != nil {
		return false
	}

	return config.Options.HideMatrix
}
