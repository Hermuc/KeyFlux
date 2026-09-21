#!/usr/bin/env python3
"""把 CFF/OTF 字体转换为 glyf (TrueType) 轮廓, 并对齐命令框 exe 的字重请求。

背景: `bin/font/font.ttf` 是 KeyFlux 命令框 (`bin/KeyFlux-CommandInput.exe`) 的
唯一字体来源, exe 硬编码请求 `DWRITE_FONT_WEIGHT_BOLD(700)` + `STYLE_NORMAL`,
并以该文件建 DirectWrite 私有字体集合。已知可用的历代字体全是 glyf;
exe 对 CFF 的加载路径无验证先例, 故 CFF 源必须先转 glyf。

用法:
    python tools/font_otf2ttf.py <source.otf> <out.ttf> [--weight 700]

依赖: fontTools (pip install fonttools)
"""

import argparse
import time

from fontTools.pens.cu2quPen import Cu2QuPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont, newTable

TRUETYPE_SIG = bytes((0x00, 0x01, 0x00, 0x00)).decode("latin-1")


def convert(src, dst, weight=700):
    t0 = time.time()
    f = TTFont(src)
    if "CFF " not in f:
        raise SystemExit(f"{src} 不含 CFF 表, 无需转换")

    glyph_set = f.getGlyphSet()  # 不能用 f["CFF "].getGlyphSet(): 新版 fontTools 已移除
    order = f.getGlyphOrder()
    print(f"glyphs = {len(order)}")

    glyf = newTable("glyf")
    glyf.glyphOrder = order
    glyf.glyphs = {}
    for i, name in enumerate(order):
        pen = TTGlyphPen(None)
        # cubic -> quadratic; reverse_direction 使外轮廓符合 TrueType 顺时针约定
        glyph_set[name].draw(Cu2QuPen(pen, max_err=1.0, reverse_direction=True))
        glyf[name] = pen.glyph()
        if (i + 1) % 10000 == 0:
            print(f"  {i + 1}/{len(order)}  ({time.time() - t0:.0f}s)")

    for g in glyf.glyphs.values():
        if g.numberOfContours:
            g.recalcBounds(glyf)  # maxp.recalc 会读 xMin, 必须先算

    f["glyf"] = glyf
    f["loca"] = newTable("loca")  # glyf 的伴随表, fontTools 不会自动创建
    del f["CFF "]
    for tag in ("VORG", "DSIG"):  # VORG 仅 CFF 用; DSIG 随改造必然失效
        if tag in f:
            del f[tag]
            print(f"dropped {tag}")

    # sfnt 签名必须从 OTTO 改为 TrueType, 否则解析器仍按 CFF 找表 -> "SFNT font table missing"
    f.sfntVersion = TRUETYPE_SIG
    f["post"].formatType = 3.0

    maxp = f["maxp"]
    maxp.tableVersion = 0x00010000
    maxp.recalc(f)
    # glyf (v1.0) 必需但 CFF 源 (v0.5) 没有的字段
    for attr, val in (("maxZones", 1), ("maxTwilightPoints", 0), ("maxStorage", 0),
                      ("maxFunctionDefs", 0), ("maxInstructionDefs", 0),
                      ("maxStackElements", 0), ("maxSizeOfInstructions", 0)):
        setattr(maxp, attr, val)

    if weight:
        fix_weight(f, weight)

    f.save(dst)
    print(f"saved {dst} in {time.time() - t0:.0f}s")
    TTFont(dst, lazy=True)  # 回读自检
    print("reload OK")


def fix_weight(f, weight):
    """元数据对齐 exe 的 BOLD(700) 请求, 否则 DirectWrite 施加 BOLDSIM 合成加粗。

    本函数被 tools/font_weight_meta.py (纯元数据对齐工具) 复用 —— 保持"只改元数据,
    不碰字形"的语义, 且不依赖本模块的 CLI 状态。改签名前先看那边的调用点。
    """
    os2, head, post = f["OS/2"], f["head"], f["post"]
    print("weight: usWeightClass %d -> %d, fsSelection 0x%04X -> " % (
        os2.usWeightClass, weight, os2.fsSelection), end="")
    os2.usWeightClass = weight
    # 清 ITALIC(0x01)/REGULAR(0x40), 置 BOLD(0x20); 保留 WWS(0x100)/USE_TYPO_METRICS(0x80)
    os2.fsSelection = (os2.fsSelection & ~0x41) | 0x20
    print("0x%04X" % os2.fsSelection)
    os2.panose.bWeight = 8
    head.macStyle = (head.macStyle & ~0x02) | 0x01
    post.italicAngle = 0
    # name 表不动: 族名取自文件, 改名有查不到的风险


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("src")
    ap.add_argument("dst")
    ap.add_argument("--weight", type=int, default=700,
                    help="目标 usWeightClass (0 = 不改元数据)")
    a = ap.parse_args()
    convert(a.src, a.dst, a.weight)


if __name__ == "__main__":
    main()
