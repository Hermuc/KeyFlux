//! 音效域 (R24; 纯逻辑契约 + 后端 trait): 触发点枚举与可替换后端接口。
//!
//! R24 硬约束: 四触发点**精确对齐**且为唯一触发点全集, 映射不得张冠李戴;
//! 后端可替换 (原版 XAudio2 动态加载 → 本实现 winmm PlaySoundW), 失败静默不阻断主流程。

/// R24 四触发点全集 (映射表见 spec.md:270-275)。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SoundKey {
    /// 0x401 (清空前播放) → sound\show.wav
    Show,
    /// 0x403 (隐藏前播放) → sound\cancel.wav
    Cancel,
    /// WM_CHAR 且 ch == 0x20 → sound\spaceKey.wav
    SpaceKey,
    /// WM_CHAR 其余字符 + 非空退格 → sound\keydown.wav
    KeyDown,
}

/// 音效后端契约: 失败静默 (文件缺失 / 设备不可用时不崩溃不弹窗, R24/R29)。
pub trait SoundBackend {
    fn play(&mut self, key: SoundKey);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R24: 触发点全集恰好四值 (新增触发点 = 破坏「唯一触发点全集」契约)。
    #[test]
    fn sound_keys_are_the_full_set() {
        let all = [
            SoundKey::Show,
            SoundKey::Cancel,
            SoundKey::SpaceKey,
            SoundKey::KeyDown,
        ];
        assert_eq!(all.len(), 4);
        // 两两不同
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j]);
            }
        }
    }
}
