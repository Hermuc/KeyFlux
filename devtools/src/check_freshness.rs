//! 「暂存二进制是否比 HEAD 新」闸门 —— 原 `tools/check-freshness.ps1` 的 Rust 移植。
//!
//! 本仓库不止一次出货过**过期**的暂存二进制（"二进制被调包"）：文件看着在，无人报错，但它
//! 早于你正在验证的 commit —— 于是 `make check` 验证的是旧生成器、发布打的是旧面板。
//! mtime-vs-HEAD 把这种静默过期变成红灯。
//!
//! 检查：每个暂存产物的 LastWriteTime(UTC) 必须**晚于** HEAD committer 时间。
//! 默认集 = 那两个 mtime 确实等于构建时间的 gitignore 产物：
//!   `bin/settings.exe`、`bin/ui/KeyFlux.Settings.exe`。
//! 刻意**不含** `bin/KeyFlux-CommandInput.exe`（入库 ⇒ checkout 会盖 mtime）与
//! `bin/AutoHotkey64.exe`（vendored，故意很旧）。缺失文件默认 skip；`-Strict` 则视为失败。
//!
//! 退出码：0 = 每个在场产物都比 HEAD 新（或无 HEAD）；1 = 至少一个过期/（strict 下）缺失。
//!
//! ## 日期处理（无 chrono）
//! 用 Howard Hinnant 的 civil↔days 算法解析 git `%cI`（严格 ISO8601）与格式化 UTC 时刻，
//! 文件 mtime 取 `SystemTime` 的 epoch 秒比较 —— 零第三方日期依赖，保持 devtools crate 轻量。

use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::util::{repo_root, run_git};

// ---------------------------------------------------------------- 日期原语（Hinnant）

/// civil (y,m,d) → 距 1970-01-01 的天数。
fn days_from_civil(mut y: i64, m: i64, d: i64) -> i64 {
    y -= if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// 距 epoch 的天数 → civil (y,m,d)。
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = mp + if mp < 10 { 3 } else { -9 }; // [1, 12]
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

fn floor_div(a: i64, b: i64) -> i64 {
    let q = a / b;
    if (a % b != 0) && ((a < 0) != (b < 0)) {
        q - 1
    } else {
        q
    }
}

/// epoch 秒（UTC）→ "yyyy-MM-ddTHH:mm:ssZ"。
fn fmt_utc(epoch: i64) -> String {
    let days = floor_div(epoch, 86400);
    let rem = epoch - days * 86400; // [0, 86399]
    let (y, m, d) = civil_from_days(days);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// 解析 git `%cI` 严格 ISO8601（`YYYY-MM-DDTHH:MM:SS(Z|±HH:MM)`）→ epoch 秒（UTC）。
fn parse_iso8601(s: &str) -> Option<i64> {
    let s = s.trim();
    let (datetime, tz) = split_tz(s)?;
    let mut dp = datetime.split('T');
    let date = dp.next()?;
    let time = dp.next()?;
    let mut dparts = date.split('-');
    let y: i64 = dparts.next()?.parse().ok()?;
    let mo: i64 = dparts.next()?.parse().ok()?;
    let d: i64 = dparts.next()?.parse().ok()?;
    let mut tparts = time.split(':');
    let h: i64 = tparts.next()?.parse().ok()?;
    let mi: i64 = tparts.next()?.parse().ok()?;
    let sec: i64 = tparts.next()?.split('.').next()?.parse().ok()?;

    let mut epoch = days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + sec;
    // 时区偏移：local = utc + off ⇒ utc = local - off
    let off = tz_offset_secs(tz)?;
    epoch -= off;
    Some(epoch)
}

/// 从 ISO8601 串尾部切出 (datetime, tz)。tz = "Z" 或 "±HH:MM"。
fn split_tz(s: &str) -> Option<(&str, &str)> {
    if let Some(rest) = s.strip_suffix('Z') {
        return Some((rest, "Z"));
    }
    // 时区在时间部分之后；日期里的 '-' 不算。定位 'T' 后再找第一个 '+'/'-'。
    let t = s.find('T')?;
    for (idx, ch) in s.char_indices() {
        if idx > t && (ch == '+' || ch == '-') {
            return Some((&s[..idx], &s[idx..]));
        }
    }
    None
}

/// 时区串 → 偏移秒。"Z" → 0；"±HH:MM" → ±(HH*3600+MM*60)。
fn tz_offset_secs(tz: &str) -> Option<i64> {
    if tz == "Z" {
        return Some(0);
    }
    let (sign, rest) = tz.split_at(1);
    let sign = match sign {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let mut hm = rest.split(':');
    let hh: i64 = hm.next()?.parse().ok()?;
    let mm: i64 = hm.next().unwrap_or("0").parse().ok()?;
    Some(sign * (hh * 3600 + mm * 60))
}

fn file_mtime_epoch(path: &Path) -> Option<i64> {
    let m = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(m.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64)
}

/// CLI 入口。`args` 支持 `--path <p>`（可多次）/ `-Strict` / `--ref <iso>`。
#[allow(clippy::too_many_lines)]
pub fn run(args: &[String]) -> i32 {
    let mut paths: Vec<String> = Vec::new();
    let mut strict = false;
    let mut ref_arg: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--path" | "-Path" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    paths.push(v.clone());
                }
            }
            "--ref" | "-Ref" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    ref_arg = Some(v.clone());
                }
            }
            "-Strict" | "--strict" => strict = true,
            _ => paths.push(args[i].clone()),
        }
        i += 1;
    }

    let repo = repo_root();
    if paths.is_empty() {
        paths = vec![
            "bin/settings.exe".to_string(),
            "bin/ui/KeyFlux.Settings.exe".to_string(),
        ];
    }

    // 参考时刻 = HEAD committer date（UTC）。无 commit ⇒ 无可比对，跳过。
    let ref_epoch = match &ref_arg {
        Some(r) if !r.is_empty() => match parse_iso8601(r) {
            Some(e) => e,
            None => {
                eprintln!("[FAIL] 无法解析 -Ref: {r}");
                return 1;
            }
        },
        _ => {
            let Some(head) = run_git(&repo, &["log", "-1", "--format=%cI"]) else {
                println!("[freshness] no git HEAD available -- skipped");
                return 0;
            };
            if head.is_empty() {
                println!("[freshness] no git HEAD available -- skipped");
                return 0;
            }
            match parse_iso8601(&head) {
                Some(e) => e,
                None => {
                    println!("[freshness] no git HEAD available -- skipped");
                    return 0;
                }
            }
        }
    };

    println!("[freshness] reference (HEAD) = {}", fmt_utc(ref_epoch));
    let mut stale: Vec<String> = Vec::new();
    for rel in &paths {
        let p = repo.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !p.exists() {
            if strict {
                stale.push(format!(
                    "{rel} : MISSING (staged artifact not built) -- run: make buildClientReactor"
                ));
            } else {
                println!("  [skip] {rel} (not present)");
            }
            continue;
        }
        let Some(m) = file_mtime_epoch(&p) else {
            println!("  [skip] {rel} (mtime unreadable)");
            continue;
        };
        if m < ref_epoch {
            let mins = ((ref_epoch - m) as f64 / 60.0).round() as i64;
            stale.push(format!(
                "{rel} : STALE (mtime {} is {mins} min older than HEAD) -- rebuild before check/deploy",
                fmt_utc(m)
            ));
        } else {
            println!("  [ok] {rel}  mtime={}", fmt_utc(m));
        }
    }

    if !stale.is_empty() {
        for s in &stale {
            println!("[FAIL] {s}");
        }
        eprintln!(
            "[FAIL] freshness: {} stale staged artifact(s) -- rebuild (make buildClientReactor / make out) first",
            stale.len()
        );
        return 1;
    }
    println!("[freshness] OK -- staged artifacts are newer than HEAD");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_civil_roundtrip() {
        // 1970-01-01 = day 0
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2026-10-08
        let d = days_from_civil(2026, 10, 8);
        assert_eq!(civil_from_days(d), (2026, 10, 8));
    }

    #[test]
    fn fmt_utc_known_instant() {
        // 2026-10-08T21:55:00Z 的 epoch
        let e = days_from_civil(2026, 10, 8) * 86400 + 21 * 3600 + 55 * 60;
        assert_eq!(fmt_utc(e), "2026-10-08T21:55:00Z");
    }

    #[test]
    fn parse_iso8601_with_offset_and_z() {
        // +08:00 ⇒ UTC 减 8 小时
        let e = parse_iso8601("2026-10-08T21:55:00+08:00").unwrap();
        assert_eq!(fmt_utc(e), "2026-10-08T13:55:00Z");
        let z = parse_iso8601("2026-10-08T13:55:00Z").unwrap();
        assert_eq!(e, z);
    }

    #[test]
    fn parse_iso8601_negative_offset() {
        let e = parse_iso8601("2026-10-08T05:00:00-05:00").unwrap();
        assert_eq!(fmt_utc(e), "2026-10-08T10:00:00Z");
    }

    #[test]
    fn tz_offset_parsing() {
        assert_eq!(tz_offset_secs("Z"), Some(0));
        assert_eq!(tz_offset_secs("+08:00"), Some(8 * 3600));
        assert_eq!(tz_offset_secs("-05:30"), Some(-(5 * 3600 + 30 * 60)));
    }
}
