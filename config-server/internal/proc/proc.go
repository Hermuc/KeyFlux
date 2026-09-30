// Package proc 提供共享的子进程启动工具。
// 被 cmd/settings/main.go 与 internal/server 共同引用,
// 避免 HTTP handler 层与入口层之间的循环依赖。
package proc

import (
	"errors"
	"log"
	"os"
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

// relayTarget 是无参数降级启动的**纯决策函数**: 目标确实存在才返回可交给 explorer.exe
// 中转的绝对路径 (ok=true); 目标不存在 (或是目录) 返回 ("", false)。
//
// 由来 (用户报「保存设置后成批弹出『文档』资源管理器窗口」): explorer.exe 收到一个
// **不存在的路径**时会把它当文件夹打开, 转而弹出默认目录「文档」—— 每调用一次弹一个
// 窗口。故目标不存在时调用方必须**不要**调用 explorer, 直接按启动失败返回
// (引擎缺失/路径错误只会得到 restartFailed=true, 绝不产生打开文件夹的副作用)。
//
// 路径口径: filepath.Abs(filepath.Join(dir, exe)) —— 与 Rust relay_target 同构 (Join 内含 Clean)。
func relayTarget(dir, exe string) (string, bool) {
	absExe, err := filepath.Abs(filepath.Join(dir, exe))
	if err != nil {
		return "", false
	}
	info, err := os.Stat(absExe)
	if err != nil || info.IsDir() {
		return "", false
	}
	return absExe, true
}

// FallbackExecCmd: breakaway 失败后的降级启动。
// 无参数调用 (保存设置后的托盘重启) 改经 explorer.exe 中转: explorer 不在本进程的
// Job 层级内, 由它拉起的进程彻底脱离任何 Job, 保证托盘不被设置窗口关闭连带终止;
// 代价是目标不继承本进程的提权状态 (由 KeyFlux 启动器自行 RunAs 提权)。
// 目标存在时才中转 (relayTarget); 不存在则按启动失败返回, 绝不调用 explorer
// (否则 explorer 会把不存在的路径当文件夹打开, 弹出默认目录「文档」)。
// 带参数调用 (WindowSpy/GenerateShortcuts 等短暂工具进程) 保持普通启动。
func FallbackExecCmd(dir, exe string, args []string) bool {
	if len(args) == 0 {
		absExe, ok := relayTarget(dir, exe)
		if !ok {
			log.Println("execCmd: 未找到", exe, ", 跳过 explorer 中转")
			return false
		}
		c := exec.Command("explorer.exe", absExe)
		err := c.Start()
		if err == nil {
			return true
		}
		log.Println("execCmd: explorer 中转启动", exe, "失败:", err)
	}
	c := exec.Command(exe, args...)
	c.Dir = dir
	if err := c.Start(); err != nil {
		log.Println("execCmd: 启动", exe, "失败:", err)
		return false
	}
	return true
}

// StopProcessByName 按镜像名强制结束进程 (Windows: `taskkill /F /IM <name>`)。
//
// 用途: 命令框 (`KeyFlux-CommandInput.exe`) 在**启动时只读一次** `bin/font/font.ttf`
// (DirectWrite 私有字体集合在进程内常驻), 故换字体后必须结束旧进程才能让新字体生效。
// 该进程**懒加载** —— 引擎只在用户下次唤起命令框时重建它, 因此结束它不会影响引擎
// 或其他功能, 也无需立即重启。
//
// 进程不存在时视为成功 (幂等): taskkill 在找不到镜像时返回退出码 128, 这里显式放行,
// 避免"用户从未唤起过命令框"这类正常情形被当成失败。
func StopProcessByName(name string) bool {
	err := exec.Command("taskkill", "/F", "/IM", name).Run()
	if err == nil {
		return true
	}
	var ee *exec.ExitError
	if errors.As(err, &ee) && ee.ExitCode() == 128 {
		return true // 无此进程, 幂等成功
	}
	log.Println("StopProcessByName:", name, "结束失败:", err)
	return false
}
