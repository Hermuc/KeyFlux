//! devtools 各子命令共用的原语 —— 等价 PS 侧 `tools/lib/kf-tools.ps1` 的 helper 子集。
//!
//! 只收编被移植脚本真正用到的：repo-root 解析、SHA256（大写十六进制，= `Get-FileHash`
//! 口径）、以及 git / curl / robocopy 的外壳调用（这些外部命令在 PS 版里也是直接 shell out，
//! 移植后保持一致，避免引入 git2 / ureq+rustls 等重型依赖）。

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// 仓库根：`KEYFLUX_REPO_ROOT` 环境变量优先（对齐 `Get-KfRepoRoot` 的 override），
/// 否则取当前工作目录（devtools 由 make 从仓库根调用，故 cwd 即根）。
pub fn repo_root() -> PathBuf {
    if let Ok(v) = std::env::var("KEYFLUX_REPO_ROOT")
        && !v.is_empty()
    {
        let p = PathBuf::from(v);
        return std::fs::canonicalize(&p).unwrap_or(p);
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// 字节序列的 SHA256，大写十六进制（= PowerShell `Get-FileHash -Algorithm SHA256` 的 `.Hash`）。
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        s.push_str(&format!("{b:02X}"));
    }
    s
}

/// 文件内容的 SHA256（大写十六进制）。
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(sha256_hex(&bytes))
}

/// 运行 `git`，返回 trim 后的 stdout；非零退出或启动失败返回 None。
pub fn run_git(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// 用系统 `curl` GET 一个 URL（带 User-Agent），返回响应体文本。
/// 刻意不引 HTTP/TLS 库 —— curl 在 Windows 10+ 与 CI windows runner 均内置。
pub fn curl_get(url: &str, user_agent: &str) -> Result<String, String> {
    let out = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "30",
            "-H",
            &format!("User-Agent: {user_agent}"),
            url,
        ])
        .output()
        .map_err(|e| format!("curl 启动失败: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "curl exit {}: {}",
            out.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// 运行 `robocopy`，返回其退出码（0-7 = 成功，>= 8 = 失败；见 `Test-KfRobocopyOk`）。
pub fn run_robocopy(args: &[&str]) -> Result<i32, String> {
    let out = Command::new("robocopy")
        .args(args)
        .output()
        .map_err(|e| format!("robocopy 启动失败: {e}"))?;
    Ok(out.status.code().unwrap_or(8))
}

/// robocopy 结果约定：exit < 8 视为成功。
pub fn robocopy_ok(exit_code: i32) -> bool {
    exit_code < 8
}
