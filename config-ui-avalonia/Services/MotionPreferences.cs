using System;

namespace KeyFlux.Settings.Services;

/// <summary>
/// 应用内动效总闸 (进程内实时判定)。设环境变量 <c>KEYFLUX_NO_MOTION=1</c> 后,
/// 设置页不挂 .motion 类, 入场级联/分区展开等大幅动效整体跳过 (无障碍逃生口)。
///
/// 刻意【不】查询系统 SPI_GETCLIENTAREAANIMATION (2026-09-13 裁定): 本机实测该标志 = 0
/// (「在窗口内显示动画」被性能调优关闭) —— 它是遗留 Win32 性能开关, 现代合成器应用普遍不理会;
/// 若遵从它, 用户明确要求的动效在本机将全部不可见。故默认播放, 仅提供显式环境变量关闭。
/// </summary>
public static class MotionPreferences
{
    private const string DisableEnvVar = "KEYFLUX_NO_MOTION";

    /// <summary>允许播放动效 (未设 KEYFLUX_NO_MOTION 即允许)。</summary>
    public static bool AnimationsEnabled =>
        Environment.GetEnvironmentVariable(DisableEnvVar) is null;
}
