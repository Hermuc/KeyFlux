package command

// call.go —— 把 server 包的 CLI 桥注册进命令表。
//
// 单独成文件（而非改 command.go）的理由：command.go 在工作区是 CRLF，改它会产生
// 整文件级行尾噪音；包内多文件共享同一 Map，init 注册即可，行为等价。
// 依赖方向：command → server（server 不反向依赖 command，无环）。

import "settings/internal/server"

func init() {
	Map["Call"] = server.Call
}
