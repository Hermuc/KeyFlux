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

/// Go `proc.ExecCmd`：启动子进程（相对 `../` 工作目录），返回是否成功启动。
pub(crate) fn exec_cmd(exe: &str, args: &[&str]) -> bool {
    // Go 用 cmd.Dir 指定子进程工作目录，避免修改全局 cwd；路径为词法 abs("../") = Join+Clean
    let Ok(cwd) = std::env::current_dir() else {
        eprintln!("execCmd: 获取项目根目录失败");
        return false;
    };
    let dir = clean_path(&cwd.join(".."));

    match Command::new(exe)
        .args(args)
        .current_dir(&dir)
        .creation_flags(CREATE_BREAKAWAY_FROM_JOB)
        .spawn()
    {
        Ok(_) => true,
        Err(error) => {
            eprintln!("execCmd: breakaway 启动 {exe} 失败: {error}");
            probe_log(&format!(
                "direct-spawn FAILED exe={exe} args={args:?} dir={} error={error}",
                dir.display()
            ));
            fallback_exec_cmd(&dir, exe, args)
        }
    }
}

/// 诊断日志（临时）：把 spawn/relay 的真实参数与结果落到 `%TEMP%\keyflux-proc.log`。
///
/// 背景：用户报「保存设置后总弹资源管理器到 Documents」。生产复现（Call PUT /config）时
/// relay 静默成功、无弹窗 ⇒ 需在**下次真实复现**时拿到确切参数（exe / dir / abs / 结果）。
/// 诊断完成后应移除本函数与其调用点。
fn probe_log(message: &str) {
    use std::io::Write as _;
    let path = std::env::temp_dir().join("keyflux-proc.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "[pid {}] {message}", std::process::id());
    }
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
    let abs_exe = clean_path(&dir.join(exe));
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
                Ok(_) => {
                    probe_log(&format!(
                        "explorer-relay SPAWNED exe={exe} abs={} exists=true",
                        abs_exe.display()
                    ));
                    return true;
                }
                Err(error) => {
                    eprintln!("execCmd: explorer 中转启动 {exe} 失败: {error}");
                    probe_log(&format!(
                        "explorer-relay FAILED exe={exe} abs={} exists=true error={error}",
                        abs_exe.display()
                    ));
                }
            },
            None => {
                // 目标不存在：绝不调用 explorer（否则会打开默认目录「文档」），按启动失败返回。
                eprintln!("execCmd: 未找到 {exe}，跳过 explorer 中转");
                probe_log(&format!(
                    "explorer-relay SKIPPED exe={exe} dir={} reason=target-missing",
                    dir.display()
                ));
                return false;
            }
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
