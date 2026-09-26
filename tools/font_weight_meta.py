#!/usr/bin/env python3
"""把 glyf 字体的**元数据**对齐到 exe 请求的字重 (默认 BOLD/700), 不动任何字形。

为什么必须做: 命令框 exe 硬编码请求 `DWRITE_FONT_WEIGHT_BOLD(700)`。若字体原生
`usWeightClass != 700`, DirectWrite 在单 face 私有字体集合里会施加**合成加粗
(BOLDSIM, 横向撑宽)**, 中文密集笔画会糊成一团; 而对齐成 700 后 DirectWrite 视为
**精确匹配**, 不再合成 —— 笔画粗细 100% 由字形决定 (这也正是"字重档位 = 轮廓膨胀"
这条路成立的前提)。

与 tools/font_otf2ttf.py 的分工: 那个工具是 **CFF→glyf 转换** (顺带对齐元数据);
本工具是 **纯元数据对齐**, 用于源本来就是 glyf 的场合 —— 若用 otf2ttf 去处理 glyf
字体, 它会走一遍无意义的轮廓重写 (Cu2QuPen 往返) 并可能损失提示信息。

用法:
    python tools/font_weight_meta.py <src.ttf> <out.ttf> [--weight 700]

依赖: fontTools。改完会经 fontTools `save()` 重编译 —— 这是必要的, 手工改字节会让
`OS/2`/`head`/`post` 三表校验和失配 (fontTools 报 bad checksum)。
"""

import argparse
import os
import sys

# 复用 otf2ttf 的元数据对齐实现 (单一真源, 避免两处漂移)
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from font_otf2ttf import fix_weight  # noqa: E402

from fontTools.ttLib import TTFont  # noqa: E402


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("src")
    ap.add_argument("dst")
    ap.add_argument("--weight", type=int, default=700,
                    help="目标 usWeightClass (默认 700 = exe 请求的 BOLD)")
    a = ap.parse_args()

    if a.weight <= 0:
        raise SystemExit("--weight 必须 > 0")

    f = TTFont(a.src)
    if "glyf" not in f:
        raise SystemExit(
            f"{a.src} 不是 glyf 字体 (而是 CFF/OTF)。本工具只做元数据对齐; "
            f"请先用 tools/font_otf2ttf.py 做 CFF→glyf 转换。")

    os2 = f["OS/2"]
    before = (os2.usWeightClass, hex(os2.fsSelection), hex(f["head"].macStyle))
    fix_weight(f, a.weight)
    after = (os2.usWeightClass, hex(os2.fsSelection), hex(f["head"].macStyle))
    print(f"  before: usWeightClass={before[0]} fsSelection={before[1]} macStyle={before[2]}")
    print(f"  after : usWeightClass={after[0]} fsSelection={after[1]} macStyle={after[2]}")

    # 🔴 可复现构建: head.compile 里有 `if ttFont.recalcTimestamp: self.modified = timestampNow()`,
    #   默认 True 会用保存时刻覆盖 ⇒ 同输入产出不同 SHA。关闭它并用源字体原值填回
    #   (与 font_weight_prebake / font_embolden 同口径)。
    f.recalcTimestamp = False
    try:
        f["head"].modified = TTFont(a.src, lazy=True)["head"].modified
    except Exception:
        f["head"].modified = 0

    f.save(a.dst)
    # 回读自检 (与其它字体工具同口径)
    g = TTFont(a.dst, lazy=True)
    gos2 = g["OS/2"]
    assert gos2.usWeightClass == a.weight, "回读 usWeightClass 不符"
    print(f"saved {a.dst} ({os.path.getsize(a.dst)} B); reload OK; "
          f"usWeightClass={gos2.usWeightClass} fsSelection={hex(gos2.fsSelection)} "
          f"macStyle={hex(g['head'].macStyle)} glyphs={len(g.getGlyphOrder())}")


if __name__ == "__main__":
    main()
