//! 缩写备注侧车 (`abbr-hints.gen.txt`) —— 引擎侧候选列表的备注数据源。
//!
//! ## 为什么是"另一个文件"而不是写进 `KeyFlux.ahk`
//!
//! 备注要出现在命令框的候选列表里, 但引擎 (AHK) 运行时读不到 `config.json`
//! (AHK 无内置 JSON 解析, 手写解析器是长期负担)。两条出路:
//!
//! * 写进生成脚本: `CommandResolver.Register` 多带一个备注参数 ⇒ 那是
//!   **CONTRACTS §3.6 冻结签名**, 且 `KeyFlux.ahk` 字节一变 parity 基线就要重录;
//! * 写成一个**并列的派生文件** (本模块): 生成脚本一个字节不动, parity 4/4 不受影响,
//!   文件丢了最坏也只是"候选没有备注"。
//!
//! 选后者。数据真源仍是 `config.json` + `resources/i18n.json`, 本文件只是派生物。
//!
//! ## 格式
//!
//! 每行 `scope⇥abbr⇥中文⇥English`, UTF-8 无 BOM。**中英都写**: 生成时的界面语言
//! 不一定是用户以后用的语言 (切语言不会触发重新生成), 运行时由引擎按系统语言挑一列
//! (`SysLangIsChinese()`), 于是切语言不需要重新生成。
//!
//! 备注里的制表符/换行会被压成空格 —— 否则一条备注会把整行格式撑坏。

use std::collections::BTreeMap;
use std::path::Path;

use super::model::{Action, Config};

/// 侧车文件名 (与 `KeyFlux.ahk` 同目录)。
pub const FILE_NAME: &str = "abbr-hints.gen.txt";

/// 生成侧车内容。缩写为空或无任何备注时返回空串 (调用方据此**不写文件**,
/// 免得留下一个空文件让人以为坏了)。
pub fn render(config: &Config) -> String {
    let table: BTreeMap<String, Vec<Action>> = config.capslock_abbr();
    let mut out = String::new();
    for (abbr, actions) in &table {
        // 缩写键与配置里的展示口径一致: 第一个带备注的动作说了算 (与设置界面同源)
        let Some(comment) = actions
            .iter()
            .map(|action| action.comment.trim())
            .find(|comment| !comment.is_empty())
        else {
            continue;
        };
        let (zh, en) = crate::services::i18n::both(comment);
        let zh = sanitize(&zh);
        let en = sanitize(&en);
        if abbr.is_empty() || (zh.is_empty() && en.is_empty()) {
            continue;
        }
        // scope 固定 capslock: 分号域尚未接建议层 (见 FuzzyStrategy 的同类说明)
        out.push_str(&format!("capslock\t{abbr}\t{zh}\t{en}\n"));
    }
    out
}

/// 写侧车到 `dir` (与生成脚本同目录)。内容为空则删掉旧文件, 避免残留上一版备注。
pub fn write(config: &Config, dir: &Path) -> std::io::Result<()> {
    let path = dir.join(FILE_NAME);
    let text = render(config);
    if text.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    std::fs::write(path, text)
}

/// 制表符/换行 → 空格 (格式分隔符), 其余原样。
fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c == '\t' || c == '\r' || c == '\n' {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_collapses_separators() {
        assert_eq!(sanitize(" a\tb\nc\r "), "a b c");
    }

    #[test]
    fn render_is_line_oriented() {
        // 空配置 ⇒ 空串 (调用方据此不写文件, 免留空文件误导)
        let config = Config::default();
        assert!(render(&config).is_empty());
    }
}
