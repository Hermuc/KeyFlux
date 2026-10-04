//! R25: 资源解析基点 = 进程 exe 所在目录 (GetModuleFileNameW 后剥文件名), 与 CWD 无关。
//! 无论被引擎以相对路径启动 (CWD=bin) 还是从任意 CWD 被测试脚本启动都能加载资源。

use std::path::PathBuf;

use windows::Win32::System::LibraryLoader::GetModuleFileNameW;

/// exe 目录基点 (皮肤 / 音效 / 字体三处共用)。
pub fn exe_dir() -> PathBuf {
    let mut buf = [0u16; 1024]; // >= PATHCCH_MAX_CCH, 单次调用足够
    let n = unsafe { GetModuleFileNameW(None, &mut buf) } as usize;
    let n = n.min(buf.len());
    let full = PathBuf::from(String::from_utf16_lossy(&buf[..n]));
    full.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// R25/R26: 皮肤文件 (构造期读一次)。
pub fn skin_file(exe_dir: &std::path::Path) -> PathBuf {
    exe_dir.join("CommandInputSkin.txt")
}

/// R23/R25: 字体文件 (font\font.ttf 与真粗体面 font.bold.ttf)。
pub fn font_file(exe_dir: &std::path::Path) -> PathBuf {
    exe_dir.join("font").join("font.ttf")
}

pub fn font_bold_file(exe_dir: &std::path::Path) -> PathBuf {
    exe_dir.join("font").join("font.bold.ttf")
}

/// R24/R25: 音效文件 (sound\{name})。
pub fn sound_file(exe_dir: &std::path::Path, name: &str) -> PathBuf {
    exe_dir.join("sound").join(name)
}
