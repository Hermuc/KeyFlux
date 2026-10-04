//! bin 薄壳 (R1/R6): 全部逻辑在库形态 `cmdinput` 中; exe 仅是 `run()` 的封装。
//! 部署名 `KeyFlux-CommandInput.exe` (R5/R6: 引擎过滤串 `ahk_exe` 依赖此文件名)。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    std::process::exit(cmdinput::win::app::run());
}
