// Package proc 提供共享的子进程启动工具。
// 被 cmd/settings/main.go 与 internal/server 共同引用,
// 避免 HTTP handler 层与入口层之间的循环依赖。
package proc

import (
	"log"
	"os/exec"
	"path/filepath"
	"syscall"
)

// ExecCmd 启动子进程 (相对 ../ 工作目录); 返回是否成功启动。
// 返回值供需要感知结果的调用方使用 (如保存配置后重启 KeyFlux, 失败时经
// restartFailed 字段告知前端); 不关心结果的调用点可忽略返回值。
func ExecCmd(exe string, args ...string) bool {
	// 用 cmd.Dir 指定子进程工作目录, 避免修改全局 cwd 影响其他 goroutine (如 GetConfigHandler 读取相对路径)
	dir, err := filepath.Abs("../")
	if err != nil {
		log.Println("execCmd: 获取项目根目录失败:", err)
		return false
	}

	var c = exec.Command(exe, args...)
	c.Dir = dir
	// CREATE_BREAKAWAY_FROM_JOB: 设置界面 (Avalonia) 会将本进程置于 KILL_ON_JOB_CLOSE
	// 的 Job 中以防孤儿; 保存设置时重启的 KeyFlux 若留在 Job 内, 会在设置窗口关闭
	// (或界面进程异常退出) 时被连带终止, 表现为「保存设置后 KeyFlux 退出」。
	// 此处让拉起的进程脱离 Job; 失败时 (如外层 Job 未开放 BREAKAWAY_OK) 按场景回退。
	c.SysProcAttr = &syscall.SysProcAttr{CreationFlags: 0x01000000} // CREATE_BREAKAWAY_FROM_JOB
	if err := c.Start(); err != nil {
		log.Println("execCmd: breakaway 启动", exe, "失败:", err)
		return FallbackExecCmd(dir, exe, args)
	}
	return true
}

// FallbackExecCmd: breakaway 失败后的降级启动。
// 无参数调用 (保存设置后的托盘重启) 改经 explorer.exe 中转: explorer 不在本进程的
// Job 层级内, 由它拉起的进程彻底脱离任何 Job, 保证托盘不被设置窗口关闭连带终止;
// 代价是目标不继承本进程的提权状态 (由 KeyFlux 启动器自行 RunAs 提权)。
// 带参数调用 (WindowSpy/GenerateShortcuts 等短暂工具进程) 保持普通启动。
func FallbackExecCmd(dir, exe string, args []string) bool {
	if len(args) == 0 {
		absExe, err := filepath.Abs(filepath.Join(dir, exe))
		if err == nil {
			c := exec.Command("explorer.exe", absExe)
			if err := c.Start(); err == nil {
				return true
			}
			log.Println("execCmd: explorer 中转启动", exe, "失败:", err)
		}
	}
	c := exec.Command(exe, args...)
	c.Dir = dir
	if err := c.Start(); err != nil {
		log.Println("execCmd: 启动", exe, "失败:", err)
		return false
	}
	return true
}
