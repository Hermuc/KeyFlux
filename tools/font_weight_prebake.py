#!/usr/bin/env python3
"""把一份 glyf 字体预烘焙成 5 个字重档位文件, 供运行时按用户所选字重直接选用。

为什么是"预烘焙"而不是"运行时现算": 生成端 (settings.exe) 是 Go 程序, 仓库里
没有也无法引入轮廓布尔运算库; 而调用外部 Python 会把部署树绑死在
「目标机器必须装有 Python + fontTools + skia-pathops」上 —— 破坏可移植性。
预烘焙把这一步彻底移到**构建期**, 运行时只按档位名挑一个现成文件复制, 于是:
  - 模块化: Go 侧只认「档位 -> 候选文件名」映射, 不掺任何几何算法
  - 可移植: 部署树零外部依赖 (纯数据文件)
  - 可维护: 算法只在 tools/ 一处, 复用 font_embolden 的真布尔并集/差集实现
  - 稳定性: 运行时零计算, 无超时/无失败面
  - 安全性: 不执行任何外部脚本

档位半径 (upem=1000; 正数膨胀变粗, 负数腐蚀变细, 宽度精确变化 2r 单位):
    thin      r=-20   (腐蚀, 极细)
    light     r=-10   (腐蚀, 细)
    regular   r= 0    (原样, 不落文件 —— 生成端直接回落源字体)
    semibold  r= 14   (膨胀)
    bold      r= 28   (膨胀)

🔴 间距怎么定的 (为什么不是 0/6/10/14): 旧 4 档在 44px 下首尾总差仅 +15.8%, 且
   「中等→半粗」只有 +3.0% —— 低于 ~3.5% 的肉眼可辨阈值, 用户报障"看不出区别"。
   新 5 档实测相邻增幅 9.7%~15.8%, 首尾 +27.1%, 每档都肉眼可辨。
   ⚠ 膨胀的墨迹增长是**次线性**的 (r=6→10 只 +3.0%, 而 0→6 有 +7.3%), 因为膨胀
   会同时填窄字腔; 腐蚀则近线性 (-6.7%/6 单位)。故"每档等距"必须按**实测墨迹**
   调, 不能按半径等分。

🔴 半径上限 (两个方向不同, 都不可逆):
   · 膨胀上限 = CJK 字腔。膨胀会蚕食封闭白区 (「口」「日」「回」的内部), 过大即糊成
     实心块。实测本字体最大字腔面积保留率: r=14 -> 0.89, r=24 -> 0.80, r=28 -> 0.75,
     r=32 -> 0.72。无突变拐点, 故取 r=28 (保留 3/4) 留足余量。
     ⚠ 判定**不可用"字腔个数"**: 膨胀会在相邻笔画间新生成小封闭白口袋, 使个数先升
     后降 (本字体 r=14 时 15->16, r=20 时 18, 之后才回落), 用个数会误报。
   · 腐蚀上限 = 该字体最细笔画的半宽。过大则细笔画先锯齿、后断裂 (整段消失)。
   换字体后必须重新出预览确认, 不要盲目沿用。

🔴 变体命名必须与 Go 侧 `script.VariantPath` 完全一致:
   `<源文件名去扩展名>.<档位><扩展名>`, 例如源 `A.ttf` 的粗体变体是 `A.bold.ttf`。
   生成端按"源字体同目录 + 该命名"去找变体, 名字对不上就会静默回落源字体
   (表现为"选了字重没变化")。

安全校验 (跳过加 --no-check): 每个档位烘焙后立即光栅化复核, 任一不合规即
   删除该变体文件并跳过该档 (生成端会因此回落源字体, 不会拿到坏文件):
   · 膨胀档: **最大字腔面积**保留率 >= CAVITY_MIN_RETENTION
   · 腐蚀档: **最细笔画**墨迹保留率 >= THIN_MIN_RETENTION, 且未断成多段

用法:
    python tools/font_weight_prebake.py <src.ttf> [--outdir <dir>] [--radii -20,-10,14,32]

    不指定 --outdir 时默认写入**源字体同目录** (与生成端的查找位置一致)。

依赖: fontTools + skia-pathops + Pillow (仅用于安全校验的光栅化)。与
font_embolden.py 用同一套膨胀/腐蚀原语 (真布尔并集/差集), 差异只是本工具一次
产出多档并落成固定文件名。
"""

import argparse
import os
import shutil
import sys

# 复用 font_embolden 的变换实现 (单一算法真源, 避免两处漂移)
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from font_embolden import transform_path  # noqa: E402

from fontTools.pens.ttGlyphPen import TTGlyphPen  # noqa: E402
from fontTools.ttLib import TTFont  # noqa: E402

# 档位名 -> 半径。与 Go 侧 script.FontWeightVariants / 设置面板的 5 个档位一一对应。
# 负数 = 腐蚀 (减细), 正数 = 膨胀 (加粗), 0 = 源字体本体。
WEIGHT_RADII = {
    "thin": -20,
    "light": -10,
    "regular": 0,
    "semibold": 14,
    "bold": 28,
}

# 安全校验参数
#
# ⚠ 为什么用"字腔**面积**"而不是"字腔**个数**": 膨胀会在相邻笔画间**新生成**小的封闭
#   白口袋, 使个数先升后降 (实测本字体 r=14 时个数 15->16, r=20 时 18, 之后才回落)。
#   用个数做判据会误报 —— 早期版本就是因此把一个其实可用的档位判成失败。
#   面积则单调递减, 能真实反映"原有白腔被蚕食"的程度。
CAVITY_TEXT = "回日目田国中口"   # 含本字体最大的几个封闭白腔
CAVITY_MIN_RETENTION = 0.55      # 最大字腔面积保留率下限 (低于此判定为填死)
THIN_TEXT = "一"                 # 最细笔画的代表字
THIN_MIN_RETENTION = 0.30        # 最细笔画墨迹保留率下限 (低于此判定为几近消失)
UNSAFE_RATIO_MAX = 0.01          # 布尔运算失败(退化轮廓)的字形占比上限, 超过则丢弃该档


def _raster_components(font_path, text, size=200):
    """光栅化 text, 返回 (封闭白腔面积列表, 墨迹像素数)。

    做法: 从图像四边 flood fill 出"外部背景", 剩余未触达的白区即封闭字腔。
    面积列表降序。比按轮廓绕向判断更贴近**渲染实际观感** (用户看到的就是光栅结果)。
    """
    import collections

    from PIL import Image, ImageDraw, ImageFont

    font = ImageFont.truetype(font_path, size)
    bbox = font.getbbox(text)
    w = bbox[2] - bbox[0] + 24
    h = bbox[3] - bbox[1] + 24
    img = Image.new("L", (w, h), 0)
    ImageDraw.Draw(img).text((12 - bbox[0], 12 - bbox[1]), text, font=font, fill=255)
    px = img.load()

    ink = 0
    outside = set()
    queue = collections.deque()
    for x in range(w):
        for y in range(h):
            if px[x, y] > 128:
                ink += 1
    for x in range(w):
        for y in (0, h - 1):
            if px[x, y] <= 128 and (x, y) not in outside:
                outside.add((x, y)); queue.append((x, y))
    for y in range(h):
        for x in (0, w - 1):
            if px[x, y] <= 128 and (x, y) not in outside:
                outside.add((x, y)); queue.append((x, y))
    while queue:
        x, y = queue.popleft()
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            nx, ny = x + dx, y + dy
            if 0 <= nx < w and 0 <= ny < h and px[nx, ny] <= 128 and (nx, ny) not in outside:
                outside.add((nx, ny)); queue.append((nx, ny))

    areas = []
    seen = set(outside)
    for y in range(h):
        for x in range(w):
            if px[x, y] <= 128 and (x, y) not in seen:
                area = 0
                seen.add((x, y)); queue.append((x, y))
                while queue:
                    cx, cy = queue.popleft(); area += 1
                    for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                        nx, ny = cx + dx, cy + dy
                        if (0 <= nx < w and 0 <= ny < h and px[nx, ny] <= 128
                                and (nx, ny) not in seen):
                            seen.add((nx, ny)); queue.append((nx, ny))
                areas.append(area)
    return sorted(areas, reverse=True), ink


def _raster_horizontal_runs(font_path, text, size=200):
    """返回 text 在**任意一行**上的最大连续墨迹段数。

    笔画断裂检测: 「一」是单段横画, 完整时每行最多 1 段; 若腐蚀过度导致中间断开,
    会出现某行有 >=2 段的情况。
    """
    from PIL import Image, ImageDraw, ImageFont

    font = ImageFont.truetype(font_path, size)
    bbox = font.getbbox(text)
    img = Image.new("L", (bbox[2] - bbox[0] + 24, bbox[3] - bbox[1] + 24), 0)
    ImageDraw.Draw(img).text((12 - bbox[0], 12 - bbox[1]), text, font=font, fill=255)
    px = img.load()
    worst = 0
    for y in range(img.height):
        runs = 0
        prev = False
        for x in range(img.width):
            cur = px[x, y] > 128
            if cur and not prev:
                runs += 1
            prev = cur
        worst = max(worst, runs)
    return worst


def check_safety(src, dst, radius):
    """烘焙后复核 dst 是否可用。返回 (ok, 说明)。不合规的档位必须被丢弃。

    判据见模块 docstring「安全校验」:
      · 膨胀: 最大字腔面积保留率 >= CAVITY_MIN_RETENTION
      · 腐蚀: 最细笔画墨迹保留率 >= THIN_MIN_RETENTION, 且未断成多段
    """
    base_areas, _ = _raster_components(src, CAVITY_TEXT)
    areas, _ = _raster_components(dst, CAVITY_TEXT)

    if radius > 0:
        if not base_areas or not areas:
            return False, "无法测量字腔"
        keep = areas[0] / base_areas[0]
        if keep < CAVITY_MIN_RETENTION:
            return False, (f"最大字腔面积保留 {keep:.2f} < 下限 {CAVITY_MIN_RETENTION} "
                           f"({areas[0]} / {base_areas[0]} px) —— 膨胀过度, 白腔被填死")
        return True, f"最大字腔保留 {keep:.2f} ({areas[0]}/{base_areas[0]} px)"

    if radius < 0:
        _, base_ink = _raster_components(src, THIN_TEXT)
        _, ink = _raster_components(dst, THIN_TEXT)
        runs = _raster_horizontal_runs(dst, THIN_TEXT)
        keep = ink / base_ink if base_ink else 0.0
        if keep < THIN_MIN_RETENTION:
            return False, (f"最细笔画墨迹保留 {keep:.2f} < 下限 {THIN_MIN_RETENTION} "
                           f"({ink} / {base_ink} px) —— 腐蚀过度, 笔画几近消失")
        if runs > 1:
            return False, f"最细笔画断成 {runs} 段 —— 腐蚀过度, 笔画断裂"
        return True, f"最细笔画保留 {keep:.2f}, 完整单段"

    return True, "原样"


def bake_one(src, dst, radius, log_unsafe=True):
    """按 radius 变换 src 后写成 dst; radius=0 时直接复制字节 (不重编译, 保持原样)。

    返回 (changed, skipped, unsafe, unsafe_names):
      changed —— 实际做了布尔变换的字形数
      skipped —— 跳过 (空字形/复合字形) 的字形数
      unsafe  —— 布尔运算失败、**保留原轮廓**的字形数
      unsafe_names —— 上述字形名列表 (便于定位/复现)

    🔴 为什么必须"失败保原样"而不是抛异常中止整档:
       skia-pathops 的布尔并集对**退化轮廓** (零面积自交/重复点堆叠) 会返回失败 ——
       实测本字体在 r=+28 时仅 `uni57F3` 一个字形触发 (r=+14 全通过)。
       若因此中止, 整档 `bold` 都拿不到, 代价与收益完全不成比例。
       单个字形保留原轮廓 = 该字形不加粗, 视觉上与邻居相差 1 档以内、
       远小于"整档缺失回落源字体"的影响; 且失败字形数会被如实回报,
       超过 UNSAFE_RATIO_MAX 才判定该档不可用 (避免"大面积退化却仍落盘")。
    """
    if radius == 0:
        shutil.copyfile(src, dst)
        return 0, 0, 0, []

    f = TTFont(src)
    if "glyf" not in f:
        raise SystemExit(f"{src} 不是 glyf 字体; CFF 源请先用 tools/font_otf2ttf.py 转换")
    glyph_set = f.getGlyphSet()
    glyf = f["glyf"]
    order = f.getGlyphOrder()

    done = skipped = unsafe = 0
    unsafe_names = []
    for name in order:
        g = glyf[name]
        if g.numberOfContours <= 0:  # 空字形 / 复合字形 (复合字形引用已变换的子字形)
            skipped += 1
            continue
        try:
            fill = _path_of(glyph_set, name)
            pen = TTGlyphPen(None)
            transform_path(fill, radius).draw(pen)
            glyf[name] = pen.glyph()
            done += 1
        except Exception as exc:  # noqa: BLE001 — pathops 对退化轮廓抛 PathOpsError
            unsafe += 1
            unsafe_names.append(name)
            if log_unsafe:
                print(f"    · 布尔运算失败, 保留原轮廓: {name} ({type(exc).__name__})")

    for name in order:
        if glyf[name].numberOfContours:
            glyf[name].recalcBounds(glyf)

    f["maxp"].recalc(f)  # 点数变化, maxPoints/maxContours 必须同步

    # 🔴 可复现构建: fontTools 的 head.compile 里有 `if ttFont.recalcTimestamp:
    #   self.modified = timestampNow()` —— 默认 True 会用**保存时刻**覆盖我们设的值,
    #   使同一输入产出不同 SHA (变体无法用 SHA 断言, 部署不可复现)。
    #   故: ① 关闭 recalcTimestamp; ② 用**源字体的 modified 原值**填回, 令输出纯粹是输入的函数。
    #   注: head.checkSumAdjustment 由 save() 末尾统一重算, 随 modified 自动归位, 无需手填。
    f.recalcTimestamp = False
    f["head"].modified = _source_modified(src)

    f.save(dst)
    TTFont(dst, lazy=True)  # 回读复核 (与 font_embolden 同口径)
    return done, skipped, unsafe, unsafe_names


def _path_of(glyph_set, name):
    import pathops

    p = pathops.Path()
    glyph_set[name].draw(p.getPen())
    return p


def _source_modified(src):
    """读取源字体的 head.modified (Mac 纪元秒), 用于烘焙时固定同值 → 可复现构建。

    源缺失 head 表 (理论上不合法) 时回落 0, 同样保证确定性。
    """
    try:
        return TTFont(src, lazy=True)["head"].modified
    except Exception:
        return 0


def variant_name(src, weight):
    """按 Go 侧 VariantPath 的约定推导变体文件名: <去扩展名>.<档位><扩展名>。"""
    base, ext = os.path.splitext(os.path.basename(src))
    return f"{base}.{weight}{ext}"


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("src")
    ap.add_argument("--outdir", default=None,
                    help="变体输出目录 (默认 = 源字体同目录, 与生成端查找位置一致)")
    ap.add_argument("--radii", default=None,
                    help="覆盖默认半径, 逗号分隔 4 个值, 顺序 thin,light,semibold,bold "
                         "(默认 -20,-10,14,32)")
    ap.add_argument("--no-check", action="store_true",
                    help="跳过安全校验 (仅调试用; 正常流程不要加)")
    a = ap.parse_args()

    radii = dict(WEIGHT_RADII)
    if a.radii:
        vals = [float(x) for x in a.radii.split(",")]
        if len(vals) != 4:
            raise SystemExit("--radii 需要恰好 4 个值 (thin,light,semibold,bold)")
        radii.update({"thin": vals[0], "light": vals[1],
                      "semibold": vals[2], "bold": vals[3]})

    outdir = a.outdir or os.path.dirname(os.path.abspath(a.src))
    os.makedirs(outdir, exist_ok=True)

    skipped_tiers = []
    for weight, r in radii.items():
        dst = os.path.join(outdir, variant_name(a.src, weight))
        if r == 0:
            print(f"[{weight}] r=0 -> 跳过落文件 (生成端直接回落源字体)")
            continue
        done, skipped, unsafe, unsafe_names = bake_one(a.src, dst, r)
        verb = "膨胀" if r > 0 else "腐蚀"
        msg = (f"[{weight}] r={r:+.0f} ({verb}) -> {dst} "
               f"(changed={done} skipped={skipped} unsafe={unsafe})")

        if a.no_check:
            print(msg)
            continue

        # 退化字形比例过高 => 该档名不副实 (大量字形实际未按档位变化), 必须丢弃。
        # 阈值取 1%: 单字形偶发失败 (实测 r=+28 仅 1/24362 ≈ 0.004%) 可接受,
        # 成片失败则说明半径对该字体过大或算法不适用。
        total_scalable = done + unsafe
        if total_scalable and unsafe / total_scalable > UNSAFE_RATIO_MAX:
            os.remove(dst)
            skipped_tiers.append(weight)
            print(f"{msg}\n    ⚠ 退化字形比例 {unsafe / total_scalable:.2%} 超上限 "
                  f"{UNSAFE_RATIO_MAX:.0%}, 已删除该变体并跳过")
            if unsafe_names:
                print(f"      示例: {', '.join(unsafe_names[:8])}")
            continue

        ok, why = check_safety(a.src, dst, r)
        if ok:
            note = f" (其中 {unsafe} 个字形保留原轮廓)" if unsafe else ""
            print(f"{msg}\n    校验通过: {why}  {os.path.getsize(dst)} B{note}")
        else:
            os.remove(dst)  # 坏变体必须删掉, 否则生成端会选中它
            skipped_tiers.append(weight)
            print(f"{msg}\n    ⚠ 校验失败, 已删除该变体并跳过: {why}")

    if skipped_tiers:
        print(f"\n⚠ 以下档位因不安全被跳过 (生成端将回落源字体): {', '.join(skipped_tiers)}")
        print("  若确需这些档位, 请换一个笔画余量更大的源字体, 或用 --radii 调小半径。")


if __name__ == "__main__":
    main()
