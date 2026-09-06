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

	proc.ExecCmd("./KeyFlux.exe", "/script", "./bin/MiscTools.ahk", "GenerateShortcuts")
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
