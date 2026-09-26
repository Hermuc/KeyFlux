#!/usr/bin/env python3
"""给 glyf (TrueType) 字体做轮廓膨胀 (笔画变粗) / 腐蚀 (笔画变细)。
—— 都是几何改形, 有别于 DirectWrite 的 BOLDSIM 合成加粗 (后者会把中文糊成一团)。

原理 (两者互为对偶):
    dilate(path, r) = path ∪ stroke(path, 2r)     变粗, 宽度 +2r 单位
    erode (path, r) = path − stroke(path, 2r)     变细, 宽度 -2r 单位
  —— 即与半径 r 的圆盘做 Minkowski 和 / 差 (ROUND_CAP/ROUND_JOIN 保证圆角)。
  upem=1000 时, 44px 下约折合 r*0.044 px 的单侧变化量。

用法:
    python tools/font_embolden.py <src.ttf> <out.ttf> --radius 12
    python tools/font_embolden.py <src.ttf> <out.ttf> --radius -12   # 负数 = 腐蚀

选半径: 先测基准竖干宽度 (upem 1000 下普通字重约 80 单位), 再按需增减。
  🔴 两个方向都有上限, 且**上限不同**:
    · 膨胀过大 (本字体实测 >= 44) -> CJK 字腔 (封闭白区) 被填死, 变成"实心块";
      本字体实测安全上限 r=36 (10 个字腔完好), r=44 起有字腔丢失。
    · 腐蚀过大 -> 细笔画先锯齿、后断裂 (细笔画整段消失)。
  建议先用 --radius 8/12/16 (或 -8/-12/-16) 出预览再定。

依赖: fontTools + skia-pathops (pip install fonttools skia-pathops)

⚠ 必须用 pathops.op(..., PathOp.UNION) 做**真布尔并集**。
  不可用 `pathops.union([a, b], pen)`: 那个函数只是把轮廓丢进同一个 Path 再
  `simplify()`, 当原字形 (TrueType 顺时针外轮廓) 与 stroke 产出的环 (绕向不同)
  混合时会误判, 把重叠区当成空洞 ⇒ 笔画渲染成空心轮廓。
  实测 (加, r=16): 真 UNION 面积 280700 -> 403497 (3 段轮廓); concat+simplify
  只得 285886 (12 段碎轮廓)。
  腐蚀同理必须用 DIFFERENCE, 不可用 simplify 近似。
"""

import argparse
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


def erode_path(fill, radius):
    """腐蚀 (笔画变细) —— dilate_path 的对偶: 把 UNION 换成 DIFFERENCE。

    erode(path, r) = path − stroke(path, 2r)
      —— 即与半径 r 的圆盘做 Minkowski 差, 笔画宽度精确减少 2r 个字体单位。

    用途: 源字体本身已是粗体 (usWeightClass=700, 命令框 exe 硬编码请求 BOLD) 时,
    "更细的档位" 只能靠腐蚀得到 —— 单纯不做膨胀只是回到源字体, 无法比源字体更细。

    ⚠ 与膨胀共享同一条硬约束: **半径上限 = 该字体最细笔画的半宽**。超过后笔画会
      先变锯齿、再断裂 (细笔画整段消失, 比字腔填死更难看)。调用方须自行验证
      (见 font_weight_prebake.py 的细笔画完整性检查), 本函数不做安全判断。
    """
    line = pathops.Path()
    fill.draw(line.getPen())
    line.stroke(2.0 * radius, pathops.LineCap.ROUND_CAP,
                pathops.LineJoin.ROUND_JOIN, 4.0)
    line.convertConicsToQuads()
    out = pathops.op(fill, line, pathops.PathOp.DIFFERENCE)
    out.convertConicsToQuads()
    return out


def transform_path(fill, radius):
    """按 radius 的符号分派: >0 膨胀变粗, <0 腐蚀变细, ==0 原样返回。

    单一分派点 —— 调用方 (CLI / font_weight_prebake) 不必自己判断方向,
    避免两处各写一遍 `if radius > 0` 而造成漂移。
    """
    if radius > 0:
        return dilate_path(fill, radius)
    if radius < 0:
        return erode_path(fill, -radius)
    return fill


def transform_glyph(glyph_set, name, radius):
    """按 radius 符号分派到膨胀/腐蚀, 返回新的 TTGlyph。

    唯一的逐字形入口 (radius 正数变粗 / 负数变细): CLI 与
    tools/font_weight_prebake.py 都走这里, 方向判断只在 transform_path 一处完成。
    """
    fill = pathops.Path()
    glyph_set[name].draw(fill.getPen())
    pen = TTGlyphPen(None)
    transform_path(fill, radius).draw(pen)
    return pen.glyph()


def _source_modified(src):
    """读取源字体 head.modified (Mac 纪元秒), 用于固定输出 → 可复现构建 (SHA 稳定)。"""
    try:
        return TTFont(src, lazy=True)["head"].modified
    except Exception:
        return 0


def _resave(src, dst, per_glyph, radius):
    """对 src 的每个轮廓字形施加 per_glyph 变换后存到 dst, 返回 (done, skipped)。"""
    f = TTFont(src)
    if "glyf" not in f:
        raise SystemExit(f"{src} 不是 glyf 字体; CFF 源请先用 tools/font_otf2ttf.py 转换")
    glyph_set = f.getGlyphSet()
    glyf = f["glyf"]
    order = f.getGlyphOrder()

    done = skipped = 0
    t0 = time.time()
    for i, name in enumerate(order):
        g = glyf[name]
        if g.numberOfContours <= 0:  # 空字形 / 复合字形 (复合字形会引用已变换的子字形)
            skipped += 1
            continue
        glyf[name] = per_glyph(glyph_set, name, radius)
        done += 1
        if (i + 1) % 5000 == 0:
            print(f"  {i + 1}/{len(order)}  ({time.time() - t0:.0f}s)", flush=True)

    for name in order:
        if glyf[name].numberOfContours:
            glyf[name].recalcBounds(glyf)

    maxp = f["maxp"]
    maxp.recalc(f)  # 点数增加, maxp 的 maxPoints/maxContours 必须同步
    print(f"transformed={done} skipped={skipped} maxPoints={maxp.maxPoints} "
          f"maxContours={maxp.maxContours}")

    # 🔴 可复现构建: head.compile 里有 `if ttFont.recalcTimestamp: self.modified = timestampNow()`,
    #   默认 True 会用保存时刻覆盖 ⇒ 同输入产出不同 SHA, 变体无法用 SHA 断言。
    #   故关闭 recalcTimestamp 并用源字体 modified 原值填回, 令输出纯粹是输入的函数。
    f.recalcTimestamp = False
    f["head"].modified = _source_modified(src)

    f.save(dst)
    print(f"saved {dst} in {time.time() - t0:.0f}s")
    TTFont(dst, lazy=True)
    print("reload OK")
    return done, skipped


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("src")
    ap.add_argument("dst")
    ap.add_argument("--radius", type=float, required=True,
                    help="膨胀半径 (字体单位; 正数变粗 / 负数变细)。"
                         "upem=1000 时 44px 下约 = |radius|*0.044 px 的单侧变化量")
    a = ap.parse_args()
    if a.radius == 0:
        raise SystemExit("radius 不能为 0 (那等于原样复制, 无意义)")
    _resave(a.src, a.dst, transform_glyph, a.radius)


if __name__ == "__main__":
    main()
