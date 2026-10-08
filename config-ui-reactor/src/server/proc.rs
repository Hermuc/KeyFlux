//! 子进程工具 —— Go `internal/proc/proc.go`（85 行）的移植。
//!
//! * [`exec_cmd`]：以 `CREATE_BREAKAWAY_FROM_JOB` 启动子进程（工作目录 = 进程 cwd
//!   的 `..`，同 Go `filepath.Abs("../")` 的**词法**口径），失败降级
//!   [`fallback_exec_cmd`]（无参数调用改经 explorer.exe 中转，使其彻底脱离本进程
//!   的 Job 层级 —— 设置面板用 KILL_ON_JOB_CLOSE 的 Job 防孤儿，保存设置时重启的
//!   KeyFlux 若不脱离会被面板关闭连带终止）。
//! * [`exec_cmd_elevated`]：runas 动词提权启动（计划任务命令 3/4 专用，见
//!   `super::server_command` 的边界说明）。
//! * [`stop_process_by_name`]：`taskkill /F /IM <name>`，退出码 128（无此进程）
//!   视为幂等成功。
//!
//! ⚠️ 相对 exe 的解析口径（2026-10-07 修复，`resolve_exe`）：std `Command::new`
//! 对相对路径按**父进程 cwd** 查找，子进程 `current_dir` 不参与解析（差分实测：
//! 父 cwd 无 cmd.exe + 子 cwd=System32 → os error 2）。服务器 cwd = `bin/`，而
//! `./KeyFlux.exe` 在部署根 —— 不预解析则命令 2/3/4 与引擎重启的 spawn 全部
//! 静默失败（重启只因 explorer 中转兜底才存活；带参命令无兜底，开机自启开关
//! 因此完全不生效）。Go 的 `exec.LookPath` 同按父 cwd 查找，即 Go 时代同病。

use std::path::{Path, PathBuf};
use std::process::Command;

use std::os::windows::process::CommandExt;

/// Go `syscall.SysProcAttr{CreationFlags: 0x01000000}`：CREATE_BREAKAWAY_FROM_JOB。
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

/// Go `filepath.Abs` 的语义 = Join + **Clean**：Clean 会消解词法上的 `.` 与 `..`
/// （`filepath.Abs("../")` 返回的是归一后的绝对路径）。Rust 的 `PathBuf::join`
/// **不做归一**（`bin\..` 原样保留）—— 必须手工 Clean，否则 explorer.exe 中转拿到
/// 含 `..` 的脏路径会把参数当不存在的文件夹处理，表现为弹出默认目录（文档）而非
/// 拉起引擎。纯词法操作（不做 symlink 解析，避免 canonicalize 的 `\\?\` 前缀）。
fn clean_path(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::ParentDir => {
                out.pop(); // 根上多余的 ".." 与 Go Clean 一致地保留为 ".."
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// 相对 exe 预解析（见模块头注 ⚠️）：`dir` 为子进程工作目录（部署根），
/// 相对 exe 必须按它归一成绝对路径后再交给 `Command::new`。
fn resolve_exe(dir: &Path, exe: &str) -> PathBuf {
    clean_path(&dir.join(exe))
}

/// 非提权启动：CREATE_BREAKAWAY_FROM_JOB（脱离面板 Job，关面板不连带终止子进程）。
fn spawn_normal(exe: &str, args: &[&str], dir: &Path) -> bool {
    let exe_path = resolve_exe(dir, exe);
    match Command::new(&exe_path)
        .args(args)
        .current_dir(dir)
        .creation_flags(CREATE_BREAKAWAY_FROM_JOB)
        .spawn()
    {
        Ok(_) => true,
        Err(error) => {
            crate::devlog!("execCmd: breakaway 启动 {exe} 失败: {error}");
            false
        }
    }
}

/// Go `proc.ExecCmd`：启动子进程（相对 `../` 工作目录），返回是否成功启动。
pub(crate) fn exec_cmd(exe: &str, args: &[&str]) -> bool {
    // Go 用 cmd.Dir 指定子进程工作目录，避免修改全局 cwd；路径为词法 abs("../") = Join+Clean
    let Ok(cwd) = std::env::current_dir() else {
        crate::devlog!("execCmd: 获取项目根目录失败");
        return false;
    };
    let dir = clean_path(&cwd.join(".."));

    if spawn_normal(exe, args, &dir) {
        return true;
    }
    fallback_exec_cmd(&dir, exe, args)
}

// --------------------------------------------------------------------------- 提权启动

/// 提权启动子进程（工作目录 = 进程 cwd 的 `..`，同 [`exec_cmd`]；FFI 见
/// [`crate::platform::elevation`]）。
///
/// 用途：计划任务命令 3/4。计划任务 KeyFlux 由提权链创建（runLevel HIGHEST），
/// 非提权上下文 `schtasks /delete` 直接「拒绝访问」（2026-10-07 实测），而
/// MiscTools.ahk 仅 On 分支内建 `*RunAs` 自提权（`bin/**` 边界零改动，无法给
/// Off 补自提权）⇒ 由本函数统一 runas 提权：面板经引擎托盘拉起时本就提权，
/// 直启（未提权）时弹一次 UAC —— 与 On 的既有 UX 对称且不会二次叠加
/// （提权后的 MiscTools 看到 `A_IsAdmin` 即真，跳过自身 `*RunAs`）。
pub(crate) fn exec_cmd_elevated(exe: &str, args: &[&str]) -> bool {
    let Ok(cwd) = std::env::current_dir() else {
        crate::devlog!("execCmdElevated: 获取项目根目录失败");
        return false;
    };
    let dir = clean_path(&cwd.join(".."));
    crate::platform::elevation::spawn_elevated(&resolve_exe(&dir, exe), args, &dir)
}

/// 无参数中转的**纯决策函数**：目标确实存在才返回可交给 explorer.exe 的绝对路径。
///
/// 由来（用户报「保存设置后成批弹出『文档』资源管理器窗口」）：explorer.exe 收到一个
/// **不存在的路径**时会把它当文件夹打开，转而弹出默认目录「文档」—— 每调用一次弹一个
/// 窗口。故目标不存在时必须返回 `None`，让调用方**不要**调用 explorer，按启动失败返回
/// （引擎缺失/路径错误只会得到 `restartFailed=true`，绝不产生打开文件夹的副作用）。
///
/// 路径口径同 Go `filepath.Abs(filepath.Join(dir, exe))`：Join 之后还要 Clean（消解 `..`）。
fn relay_target(dir: &Path, exe: &str) -> Option<PathBuf> {
    let abs_exe = resolve_exe(dir, exe);
    if abs_exe.is_file() {
        Some(abs_exe)
    } else {
        None
    }
}

/// Go `proc.FallbackExecCmd`：breakaway 失败后的降级启动。
/// 无参数调用（保存设置后的托盘重启）改经 explorer.exe 中转（**目标存在时才中转**）；
/// 带参数调用（WindowSpy 等短暂工具进程）保持普通启动，不引入 explorer。
fn fallback_exec_cmd(dir: &Path, exe: &str, args: &[&str]) -> bool {
    if args.is_empty() {
        match relay_target(dir, exe) {
            Some(abs_exe) => match Command::new("explorer.exe").arg(&abs_exe).spawn() {
                Ok(_) => return true,
                Err(error) => {
                    crate::devlog!("execCmd: explorer 中转启动 {exe} 失败: {error}");
                }
            },
            None => {
                // 目标不存在：绝不调用 explorer（否则会打开默认目录「文档」），按启动失败返回。
                crate::devlog!("execCmd: 未找到 {exe}，跳过 explorer 中转");
                return false;
            }
        }
    }
    match Command::new(resolve_exe(dir, exe))
        .args(args)
        .current_dir(dir)
        .spawn()
    {
        Ok(_) => true,
        Err(error) => {
            crate::devlog!("execCmd: 启动 {exe} 失败: {error}");
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
            crate::devlog!(
                "StopProcessByName: {name} 结束失败: exit code {:?}",
                status.code()
            );
            false
        }
        Err(error) => {
            crate::devlog!("StopProcessByName: {name} 结束失败: {error}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_path_resolves_parent_and_current() {
        // Go filepath.Abs = Join + Clean（消解 "." 与 ".."）
        let p = Path::new("D:\\a\\bin\\..");
        assert_eq!(clean_path(p), PathBuf::from("D:\\a"));
        let p = Path::new("D:\\a\\bin\\..\\./KeyFlux.exe");
        assert_eq!(clean_path(p), PathBuf::from("D:\\a\\KeyFlux.exe"));
        let p = Path::new("D:\\a\\KeyFlux.exe");
        assert_eq!(clean_path(p), PathBuf::from("D:\\a\\KeyFlux.exe"));
    }

    /// 相对 exe 必须按**目标工作目录**预解析（差分回归）：std 按父进程 cwd 查找，
    /// 而服务器 cwd = bin/，`./KeyFlux.exe` 在部署根 —— 不预解析则命令 2/3/4 与
    /// 引擎重启的 spawn 全部 os error 2（本 bug 的根因路径）。
    ///
    /// 解析断言为硬断言：cargo test 的父 cwd（crate 目录）没有 cmd.exe，只有
    /// 目标目录（System32）有 ⇒ 解析必须落在目标目录。
    #[test]
    fn spawn_resolves_relative_exe_against_target_dir() {
        let system32 = std::env::var_os("SystemRoot")
            .map(std::path::PathBuf::from)
            .map(|root| root.join("System32"))
            .expect("SystemRoot 环境变量必然存在");
        assert!(
            resolve_exe(&system32, "./cmd.exe").is_file(),
            "相对 exe 应按目标工作目录解析出真实文件"
        );
        // 反向锚点：目标目录里不存在的 exe 依旧解析不出（防测试恒真）
        assert!(!resolve_exe(&system32, "./kf-no-such-exe.exe").is_file());

        // 完整 spawn 仅作冒烟：测试进程可能处于**禁 breakaway** 的 Job（CI/沙箱），
        // 此时 CREATE_BREAKAWAY_FROM_JOB 会 os error 5 —— 生产链路的 Job 设有
        // BREAKAWAY_OK（platform::job），不受此限，故失败只记日志不误报。
        if !spawn_normal("./cmd.exe", &["/c", "exit 0"], &system32) {
            crate::devlog!("spawn 冒烟被环境 Job 策略限制，解析断言已覆盖修复口径");
        }
    }

    #[test]
    fn relay_target_requires_existing_file() {
        // 目标存在 → 可中转（返回 clean 后的绝对路径）；缺失/是目录 → 不中转。
        // 这正是「保存设置后成批弹出『文档』窗口」的守门人：目标不存在时若仍调用
        // explorer.exe，它会把不存在的路径当文件夹打开 → 弹默认目录。
        let base =
            std::env::temp_dir().join(format!("kf-relay-target-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("create temp dir");
        std::fs::write(base.join("present.exe"), b"stub").expect("write target");

        assert_eq!(
            relay_target(&base, "present.exe"),
            Some(clean_path(&base.join("present.exe")))
        );

        // 缺失的 exe → None（绝不调用 explorer）
        assert_eq!(relay_target(&base, "missing.exe"), None);

        // 同名目录不是可执行文件 → 同样 None
        std::fs::create_dir_all(base.join("adir")).expect("create subdir");
        assert_eq!(relay_target(&base, "adir"), None);

        let _ = std::fs::remove_dir_all(&base);
    }
}
