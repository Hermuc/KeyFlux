//! 子进程工具 —— Go `internal/proc/proc.go`（85 行）的移植。
//!
//! * [`exec_cmd`]：以 `CREATE_BREAKAWAY_FROM_JOB` 启动子进程（工作目录 = 进程 cwd
//!   的 `..`，同 Go `filepath.Abs("../")` 的**词法**口径），失败降级
//!   [`fallback_exec_cmd`]（无参数调用改经 explorer.exe 中转，使其彻底脱离本进程
//!   的 Job 层级 —— 设置面板用 KILL_ON_JOB_CLOSE 的 Job 防孤儿，保存设置时重启的
//!   KeyFlux 若不脱离会被面板关闭连带终止）。
//! * [`stop_process_by_name`]：`taskkill /F /IM <name>`，退出码 128（无此进程）
//!   视为幂等成功。

use std::path::{Path, PathBuf};
use std::process::Command;

use std::os::windows::process::CommandExt;

/// Go `syscall.SysProcAttr{CreationFlags: 0x01000000}`：CREATE_BREAKAWAY_FROM_JOB。
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

/// Go `proc.ExecCmd`：启动子进程（相对 `../` 工作目录），返回是否成功启动。
pub(crate) fn exec_cmd(exe: &str, args: &[&str]) -> bool {
    // Go 用 cmd.Dir 指定子进程工作目录，避免修改全局 cwd；路径为词法 abs("../")
    let Ok(cwd) = std::env::current_dir() else {
        eprintln!("execCmd: 获取项目根目录失败");
        return false;
    };
    let dir = cwd.join("..");

    match Command::new(exe)
        .args(args)
        .current_dir(&dir)
        .creation_flags(CREATE_BREAKAWAY_FROM_JOB)
        .spawn()
    {
        Ok(_) => true,
        Err(error) => {
            eprintln!("execCmd: breakaway 启动 {exe} 失败: {error}");
            fallback_exec_cmd(&dir, exe, args)
        }
    }
}

/// Go `proc.FallbackExecCmd`：breakaway 失败后的降级启动。
/// 无参数调用（保存设置后的托盘重启）改经 explorer.exe 中转；带参数调用
/// （WindowSpy 等短暂工具进程）保持普通启动。
fn fallback_exec_cmd(dir: &Path, exe: &str, args: &[&str]) -> bool {
    if args.is_empty() {
        // Go: filepath.Abs(filepath.Join(dir, exe)) —— dir 已是绝对词法路径
        let abs_exe: PathBuf = dir.join(exe);
        match Command::new("explorer.exe").arg(&abs_exe).spawn() {
            Ok(_) => return true,
            Err(error) => eprintln!("execCmd: explorer 中转启动 {exe} 失败: {error}"),
        }
    }
    match Command::new(exe).args(args).current_dir(dir).spawn() {
        Ok(_) => true,
        Err(error) => {
            eprintln!("execCmd: 启动 {exe} 失败: {error}");
            false
        }
    }
}

/// Go `proc.StopProcessByName`（proc.go:73-84）：`taskkill /F /IM <name>`，
/// 退出码 128（无此进程）视为幂等成功 —— 命令框懒加载，"从未唤起"是正常情形。
pub(crate) fn stop_process_by_name(name: &str) -> bool {
    match Command::new("taskkill").args(["/F", "/IM", name]).status() {
        Ok(status) if status.success() => true,
        Ok(status) if status.code() == Some(128) => true, // 无此进程，幂等成功
        Ok(status) => {
            eprintln!(
                "StopProcessByName: {name} 结束失败: exit code {:?}",
                status.code()
            );
            false
        }
        Err(error) => {
            eprintln!("StopProcessByName: {name} 结束失败: {error}");
            false
        }
    }
}
