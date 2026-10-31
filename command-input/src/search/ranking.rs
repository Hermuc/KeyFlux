//! 结果排序（**唯一真源**）。
//!
//! ## 为什么需要它
//!
//! Everything 的排序是"按列排"——名称 / 路径 / 大小 / 日期，全结果**平权**。
//! 搜 `cfg` 时 `config-backup-2019.zip` 与 `config.json` 谁在前面，取决于用户在
//! Everything GUI 里点的是哪一列，而不是"哪个更可能是你要的"。启动器要的恰恰是
//! 后者：最可能的那条要在第 1 行。
//!
//! ## 数据源为什么只有一个
//!
//! 唯一有真实行为支撑的信号是 Everything 自己记录的 `RUN_COUNT`（该文件被打开过
//! 几次，属性 ID 10），它随 SEARCH 的属性请求一起返回，**零额外往返**。
//! 其余线索（路径深度、文件名长度、匹配位置）只是廉价的结构性先验，用来打破平局。
//!
//! ## 两条硬约束
//!
//! 1. **不改动原有相对顺序的稳定性**：用稳定排序，分数相同时保持 Everything 给出的
//!    顺序（那是用户在 GUI 里设定的排序意愿）。同一次查询跑两遍结果必须完全一致。
//! 2. **run_count 全为 0 时必须退化得体**：新用户首日、或属性未请求到时，全部为 0，
//!    此时排序完全由结构线索决定，不引入随机性、也不至于把结果搅乱。
//!
//! 打分刻意做成**整数**：可复现、可单测、不依赖浮点平台差异。

use super::ipc::Hit;

/// 运行次数权重（每次 +20，封顶 100 次 ⇒ 上限 2000）。
const W_RUN: i64 = 20;
const RUN_CAP: u32 = 100;
/// 文件名主干与检索词完全相同（忽略大小写）。
const W_EXACT: i64 = 3000;
/// 文件名主干以检索词开头。
const W_PREFIX: i64 = 1500;
/// 文件名主干包含检索词（Everything 可能匹配的是父路径）。
const W_SUBSTR: i64 = 400;
/// 路径每深一层扣分（同等条件下浅路径更可能是常用文件）。
const W_DEPTH: i64 = 30;
/// 文件名每多一个字符扣分（短名更可能是目标）。
const W_LEN: i64 = 5;
/// 文件夹的轻惩罚：用户搜文件名时通常要的是文件。
const W_DIR: i64 = 200;

/// 单个命中项的打分。`query` 必须已归一（小写、去首尾空白）。
fn score(hit: &Hit, query: &str) -> i64 {
    if query.is_empty() {
        return 0;
    }
    let stem = stem_lower(&hit.name);
    let mut s = 0i64;

    s += i64::from(hit.run_count.min(RUN_CAP)) * W_RUN;

    if stem == query {
        s += W_EXACT;
    } else if stem.starts_with(query) {
        s += W_PREFIX;
    } else if stem.contains(query) {
        s += W_SUBSTR;
    }

    // 路径深度：`C:` (卷根) 深度 0，`C:\a\b` 深度 2。
    s -= i64::try_from(hit.path.matches('\\').count()).unwrap_or(0) * W_DEPTH;
    s -= i64::try_from(hit.name.chars().count()).unwrap_or(0) * W_LEN;
    if hit.is_dir {
        s -= W_DIR;
    }
    s
}

/// 就地排序（稳定）。分数相同时保持原顺序，见文件头约束 1。
///
/// 参数是 `&mut Vec<Hit>`（而非切片）：重排靠 `drain` + 按序 `push`，
/// 避免原地 swap 在"目标位已被占"时把元素覆盖掉。
pub fn rank(hits: &mut Vec<Hit>, query: &str) {
    let q = normalize(query);
    if q.is_empty() {
        return;
    }
    // 先算分再排序：sort_by 的比较器里重复算分会把 O(n log n) 变成 O(n log n × 成本)。
    let mut scored: Vec<(i64, usize)> = hits
        .iter()
        .enumerate()
        .map(|(i, h)| (score(h, &q), i))
        .collect();
    // 次键 = 原索引 ⇒ 同分保持原序（稳定），不依赖 sort_by 的稳定性实现细节。
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));

    let order: Vec<usize> = scored.into_iter().map(|(_, i)| i).collect();
    if order.iter().enumerate().all(|(pos, &orig)| pos == orig) {
        return; // 已是目标顺序（常见于单条/已有序），不做无谓搬动。
    }
    let mut slots: Vec<Option<Hit>> = hits.drain(..).map(Some).collect();
    for src in order {
        // order 是 0..n 的排列，每个索引恰好取一次。
        if let Some(h) = slots[src].take() {
            hits.push(h);
        }
    }
}

/// 归一到比较口径：小写 + 去首尾空白。
fn normalize(s: &str) -> String {
    s.trim().to_lowercase()
}

/// 文件名主干：去掉最后一个扩展名（`a.tar.gz` → `a.tar`），小写。
///
/// 只切最后一个点，且点不在第 0 位（`.gitignore` 这类隐藏文件主干保持原名）。
fn stem_lower(name: &str) -> String {
    let lower = name.to_lowercase();
    match lower.rfind('.') {
        Some(i) if i > 0 => lower[..i].to_string(),
        _ => lower,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(name: &str, path: &str, is_dir: bool, run: u32) -> Hit {
        Hit {
            name: name.into(),
            path: path.into(),
            is_dir,
            run_count: run,
        }
    }

    #[test]
    fn exact_beats_prefix_beats_substr() {
        let q = "config";
        let exact = h("config.json", r"D:\app", false, 0);
        let prefix = h("config-old.json", r"D:\app", false, 0);
        let substr = h("my-config.json", r"D:\app", false, 0);
        assert!(score(&exact, q) > score(&prefix, q));
        assert!(score(&prefix, q) > score(&substr, q));
    }

    #[test]
    fn run_count_can_beat_exact_match() {
        // 常用文件（打开过 100 次）应当压过"只是名字恰好相同"的陌生文件。
        let familiar = h("config-old.json", r"D:\app", false, 100);
        let stranger = h("config.json", r"D:\nowhere\deep\dir", false, 0);
        assert!(score(&familiar, "config") > score(&stranger, "config"));
    }

    #[test]
    fn shallow_path_wins_tie() {
        let shallow = h("a.txt", r"D:\x", false, 0);
        let deep = h("a.txt", r"D:\x\y\z\w", false, 0);
        assert!(score(&shallow, "a") > score(&deep, "a"));
    }

    #[test]
    fn dir_loses_to_file() {
        let file = h("src", r"D:\code", false, 0);
        let dir = h("src", r"D:\code", true, 0);
        assert!(score(&file, "src") > score(&dir, "src"));
    }

    #[test]
    fn all_zero_run_count_is_deterministic() {
        // 新用户首日：全部 run_count = 0，排序必须稳定且可复现。
        let mut a = vec![
            h("b.txt", r"D:\p", false, 0),
            h("a.txt", r"D:\p", false, 0),
            h("c.txt", r"D:\p", false, 0),
        ];
        let mut b = a.clone();
        rank(&mut a, "t");
        rank(&mut b, "t");
        let names = |v: &Vec<Hit>| v.iter().map(|h| h.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&a), names(&b), "同输入两次结果必须一致");
        // 同分同深度同长度 ⇒ 保持原始顺序（稳定性）。
        assert_eq!(names(&a), vec!["b.txt", "a.txt", "c.txt"]);
    }

    #[test]
    fn rank_is_stable_for_equal_scores() {
        let mut v = vec![
            h("x1.txt", r"D:\p", false, 0),
            h("x2.txt", r"D:\p", false, 0),
            h("x3.txt", r"D:\p", false, 0),
        ];
        rank(&mut v, "x");
        assert_eq!(
            v.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(),
            vec!["x1.txt", "x2.txt", "x3.txt"]
        );
    }

    #[test]
    fn rank_moves_frequent_to_front() {
        let mut v = vec![
            h("config-backup.zip", r"D:\old", false, 0),
            h("config.json", r"D:\app", false, 57),
        ];
        rank(&mut v, "config");
        assert_eq!(v[0].name, "config.json");
    }

    #[test]
    fn empty_query_keeps_order() {
        let mut v = vec![h("b", r"D:", false, 0), h("a", r"D:", false, 9)];
        rank(&mut v, "   ");
        assert_eq!(v[0].name, "b", "空查询不得重排");
    }

    #[test]
    fn stem_handles_multi_dot_and_hidden() {
        assert_eq!(stem_lower("a.tar.gz"), "a.tar");
        assert_eq!(stem_lower(".gitignore"), ".gitignore");
        assert_eq!(stem_lower("Config.JSON"), "config");
    }
}
