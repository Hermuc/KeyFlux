#!/usr/bin/env python3
"""给 glyf (TrueType) 字体做轮廓膨胀, 使笔画变粗 (几何加粗, 非 DirectWrite 合成加粗)。

原理: dilate(path, r) = path ∪ stroke(path, 2r)
  —— 即与半径 r 的圆盘做 Minkowski 和 (ROUND_CAP/ROUND_JOIN 保证圆角)。
  笔画宽度精确增加 2r 个字体单位 (upem=1000 时, 44px 下约增加 2r*0.044 px)。

用法:
    python tools/font_embolden.py <src.ttf> <out.ttf> --radius 12

选半径: 先测基准竖干宽度 (upem 1000 下普通字重约 80 单位), 再按需增加。
  半径过大 (本字体实测 >= 16) 会把 CJK 的字腔 (封闭白区) 填死, 变成"实心块"。
  建议先用 --radius 8/12/16 出预览再定。

依赖: fontTools + skia-pathops (pip install fonttools skia-pathops)

⚠ 必须用 pathops.op(..., PathOp.UNION) 做**真布尔并集**。
  不可用 `pathops.union([a, b], pen)`: 那个函数只是把轮廓丢进同一个 Path 再
  `simplify()`, 当原字形 (TrueType 顺时针外轮廓) 与 stroke 产出的环 (绕向不同)
  混合时会误判, 把重叠区当成空洞 ⇒ 笔画渲染成空心轮廓。
  实测 (加, r=16): 真 UNION 面积 280700 -> 403497 (3 段轮廓); concat+simplify
  只得 285886 (12 段碎轮廓)。
"""

import argparse
import sys
import time

import pathops
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont


def dilate_path(fill, radius):
    line = pathops.Path()
    fill.draw(line.getPen())
    line.stroke(2.0 * radius, pathops.LineCap.ROUND_CAP,
                pathops.LineJoin.ROUND_JOIN, 4.0)
    line.convertConicsToQuads()  # Skia stroke 会产出 CONIC 段, 布尔运算不接受
    out = pathops.op(fill, line, pathops.PathOp.UNION)
    out.convertConicsToQuads()
    return out


def dilate_glyph(glyph_set, name, radius):
    fill = pathops.Path()
    glyph_set[name].draw(fill.getPen())
    pen = TTGlyphPen(None)
    dilate_path(fill, radius).draw(pen)
    return pen.glyph()


def embolden(src, dst, radius):
    t0 = time.time()
    f = TTFont(src)
    if "glyf" not in f:
        raise SystemExit(f"{src} 不是 glyf 字体; CFF 源请先用 tools/font_otf2ttf.py 转换")
    glyph_set = f.getGlyphSet()
    glyf = f["glyf"]
    order = f.getGlyphOrder()

    done = skipped = 0
    for i, name in enumerate(order):
        g = glyf[name]
        if g.numberOfContours <= 0:  # 空字形 / 复合字形 (复合字形会引用已加粗的子字形)
            skipped += 1
            continue
        glyf[name] = dilate_glyph(glyph_set, name, radius)
        done += 1
        if (i + 1) % 5000 == 0:
            print(f"  {i + 1}/{len(order)}  ({time.time() - t0:.0f}s)", flush=True)

    for name in order:
        if glyf[name].numberOfContours:
            glyf[name].recalcBounds(glyf)

    maxp = f["maxp"]
    maxp.recalc(f)  # 点数增加, maxp 的 maxPoints/maxContours 必须同步
    print(f"dilated={done} skipped={skipped} maxPoints={maxp.maxPoints} "
          f"maxContours={maxp.maxContours}")

    f.save(dst)
    print(f"saved {dst} in {time.time() - t0:.0f}s")
    TTFont(dst, lazy=True)
    print("reload OK")


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("src")
    ap.add_argument("dst")
    ap.add_argument("--radius", type=float, required=True,
                    help="膨胀半径 (字体单位, upem=1000 时 44px 下约 = radius*0.044 px)")
    a = ap.parse_args()
    if a.radius <= 0:
        raise SystemExit("radius 必须 > 0")
    embolden(a.src, a.dst, a.radius)


if __name__ == "__main__":
    main()
