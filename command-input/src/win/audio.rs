//! R24: winmm `PlaySoundW` 音效后端 (SoundBackend 实现)。
//!
//! - `PlaySoundW(完整路径, NULL, SND_FILENAME | SND_ASYNC | SND_NODEFAULT)`:
//!   SND_NODEFAULT 保证文件缺失 / 设备不可用时**静默无声** (R24 优雅降级, 不崩溃不弹窗);
//!   SND_ASYNC 不阻塞 WndProc;
//! - 静态链接 winmm 不违反 R24「不应静态硬链接音频库」的本意 (那条针对 xaudio2_9
//!   这类版本相关 DLL; winmm 是系统常驻组件无版本问题 —— design A §3.2 口径;
//!   设计文档预留的后端可换性由 core::sound::SoundBackend trait 保证, 换 XAudio2
//!   动态加载实现不动其他层)。

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};

use crate::sound::{SoundBackend, SoundKey};
use crate::win::resources;

/// winmm 后端: 构造期解析四条完整路径 (R25: exe 目录基点)。
pub struct WinmmSound {
    /// [show, cancel, spaceKey, keydown] 完整路径 (NUL 结尾 UTF-16)
    files: [Vec<u16>; 4],
}

impl WinmmSound {
    pub fn new(exe_dir: &Path) -> Self {
        let wide_path = |name: &str| -> Vec<u16> {
            resources::sound_file(exe_dir, name)
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect()
        };
        Self {
            files: [
                wide_path("show.wav"),
                wide_path("cancel.wav"),
                wide_path("spaceKey.wav"),
                wide_path("keydown.wav"),
            ],
        }
    }
}

impl SoundBackend for WinmmSound {
    fn play(&mut self, key: SoundKey) {
        let idx = match key {
            SoundKey::Show => 0,
            SoundKey::Cancel => 1,
            SoundKey::SpaceKey => 2,
            SoundKey::KeyDown => 3,
        };
        let path = &self.files[idx];
        // 返回值不检查: PlaySound 失败本就静默 (SND_NODEFAULT), R24
        // SAFETY: PlaySoundW 只读一个 NUL 结尾宽字符串（不保留指针）；`path` 在构造期
        // 已保证末尾为 0（wide_path 链上 `once(0)`），且借用覆盖整个调用期间。
        unsafe {
            let _ = PlaySoundW(
                windows::core::PCWSTR::from_raw(path.as_ptr()),
                None,
                SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
            );
        }
    }
}
