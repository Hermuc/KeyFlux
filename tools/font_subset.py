#!/usr/bin/env python3
"""按 Unicode 区块**裁剪字体字符集**, 移除命令框用不到的区段, 显著缩小体积。

为什么需要: 更纱黑体 (Sarasa Gothic) 单 face 抽出来仍有 ~45 MB (含 2 万+ 生僻汉字:
CJK 扩展A/扩展B 等), 远超命令框字体的体积上限。而命令框**只显示用户输入的短文本**,
不可能用到这些冷僻区段。

保留策略 (默认, 覆盖命令框全部实际用例):
  基本拉丁 / 拉丁补充 / 拉丁扩展A-B / 通用标点 / 上下标 /
  箭头 / 数学运算符 / 制表符 / 几何图形 / 杂项符号 /
  CJK 符号与标点 / 平假名 / 片假名 / 注音 / 希腊 / 西里尔 /
  CJK 统一汉字**基本区** (U+4E00-9FFF, 20992 字, 日常中文全覆盖) /
  全角半角形式 / 中日韩兼容标点

**默认排除** (占体积的大头, 命令框用不到):
  CJK 扩展A (U+3400-4DBF) / 扩展B+ (U+20000+) / CJK 兼容汉字 (U+F900-FAFF) 等生僻区

用法:
  python3 tools/font_subset.py <src.ttf> --out <dst.ttf>
  python3 tools/font_subset.py <src.ttf> --out <dst.ttf> --keep-ext-a   # 保留扩展A
  python3 tools/font_subset.py <src.ttf> --out <dst.ttf> --extra "U+2600-26FF"

依赖: fontTools (subset 子模块)。
"""

import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from fontTools import subset  # noqa: E402
from fontTools.ttLib import TTFont  # noqa: E402

# 保留区段 (name, 起, 止) —— 覆盖命令框全部实际显示需求
_KEEP_RANGES = [
    ("基本拉丁", 0x0000, 0x007F),
    ("拉丁补充", 0x0080, 0x00FF),
    ("拉丁扩展A", 0x0100, 0x017F),
    ("拉丁扩展B", 0x0180, 0x024F),
    ("IPA 扩展", 0x0250, 0x02AF),
    ("修饰字母", 0x02B0, 0x02FF),
    ("组合附加符号", 0x0300, 0x036F),
    ("希腊", 0x0370, 0x03FF),
    ("西里尔", 0x0400, 0x04FF),
    ("通用标点", 0x2000, 0x206F),
    ("上下标", 0x2070, 0x209F),
    ("货币符号", 0x20A0, 0x20CF),
    ("字母式符号", 0x2100, 0x214F),
    ("数字形式", 0x2150, 0x218F),
    ("箭头", 0x2190, 0x21FF),
    ("数学运算符", 0x2200, 0x22FF),
    ("技术符号", 0x2300, 0x23FF),
    ("制表符", 0x2500, 0x257F),
    ("块元素", 0x2580, 0x259F),
    ("几何图形", 0x25A0, 0x25FF),
    ("杂项符号", 0x2600, 0x26FF),
    ("装饰符号", 0x2700, 0x27BF),
    ("中日韩符号与标点", 0x3000, 0x303F),
    ("平假名", 0x3040, 0x309F),
    ("片假名", 0x30A0, 0x30FF),
    ("注音符号", 0x3100, 0x312F),
    ("谚文兼容字母", 0x3130, 0x318F),
    ("CJK 统一汉字(基本区)", 0x4E00, 0x9FFF),
    ("CJK 兼容标点(含竖排)", 0xFE10, 0xFE1F),
    ("CJK 兼容形式", 0xFE30, 0xFE4F),
    ("小写变体形式", 0xFE50, 0xFE6F),
    ("半角全角形式", 0xFF00, 0xFFEF),
    ("特殊符号", 0xFFF0, 0xFFFF),
]

# 可选保留 (默认排除): 占体积大头
_OPTIONAL_EXT_A = ("CJK 扩展A", 0x3400, 0x4DBF)
_OPTIONAL_COMPAT = ("CJK 兼容汉字", 0xF900, 0xFAFF)


def _fmt_ranges(ranges):
    return [f"U+{lo:04X}-{hi:04X}" for _n, lo, hi in ranges]


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("src", help="源字体 (.ttf, 须为 glyf 轮廓)")
    ap.add_argument("--out", required=True, help="输出 .ttf 路径")
    ap.add_argument("--keep-ext-a", action="store_true",
                    help="额外保留 CJK 扩展A (U+3400-4DBF, 约 +4.1 MB)")
    ap.add_argument("--keep-compat", action="store_true",
                    help="额外保留 CJK 兼容汉字 (U+F900-FAFF)")
    ap.add_argument("--extra", default=None,
                    help="额外保留的区段, 逗号分隔 (如 \"U+2600-26FF,U+1F300-1F5FF\")")
    a = ap.parse_args()

    if not os.path.isfile(a.src):
        raise SystemExit(f"源文件不存在: {a.src}")

    ranges = list(_KEEP_RANGES)
    if a.keep_ext_a:
        ranges.append(_OPTIONAL_EXT_A)
    if a.keep_compat:
        ranges.append(_OPTIONAL_COMPAT)
    extra = []
    if a.extra:
        extra = [s.strip() for s in a.extra.split(",") if s.strip()]

    unicodes = _fmt_ranges(ranges) + extra

    src_size = os.path.getsize(a.src)
    f = TTFont(a.src, lazy=True)
    tags = set(f.reader.keys())
    if "glyf" not in tags:
        raise SystemExit(f"{a.src} 不是 glyf 字体 (命令框只接受 glyf); "
                         f"请先用 tools/font_otf2ttf.py 转换。")
    src_glyphs = len(f.getGlyphOrder())
    src_cmap = len(f.getBestCmap())

    args = [
        a.src,
        f"--output-file={a.out}",
        "--unicodes=" + ",".join(unicodes),
        # 保留布局与命名等必要表; 丢弃与命令框无关的大表
        "--layout-features=*",
        "--name-IDs=*",
        "--name-legacy",
        "--name-languages=*",
        "--glyph-names",
        "--notdef-outline",           # .notdef 保留轮廓, 缺字时可见方框而非崩溃
        "--recalc-bounds",
        # 丢弃 hinting 与竖排表 (命令框走 DirectWrite, 不用 ttf hinting; 不竖排)
        "--no-hinting",
        "--drop-tables+=vhea,vmtx,VORG,DSIG",
    ]
    subset.main(args)

    out_size = os.path.getsize(a.out)
    g = TTFont(a.out, lazy=True)
    out_glyphs = len(g.getGlyphOrder())
    out_cmap = len(g.getBestCmap())
    print()
    print(f"源  : {a.src}")
    print(f"      {src_size:,} B ({src_size/1048576:.1f} MiB)  字形={src_glyphs}  cmap={src_cmap}")
    print(f"输出: {a.out}")
    print(f"      {out_size:,} B ({out_size/1048576:.1f} MiB)  字形={out_glyphs}  cmap={out_cmap}")
    print(f"      体积 {out_size*100/src_size:.1f}%  (省 {(src_size-out_size)/1048576:.1f} MiB)")
    print(f"      轮廓 glyf={'glyf' in set(g.reader.keys())}  "
          f"usWeightClass={g['OS/2'].usWeightClass}")

    # 关键字符覆盖复核 (命令框真实用到的)
    cm = g.getBestCmap()
    probe = "中文测试命令框搜索文件保存取消ABCabc0123.,，。！？（）【】「」"
    miss = [ch for ch in probe if ord(ch) not in cm]
    print(f"      关键字符覆盖: {'全命中' if not miss else '缺 ' + ''.join(miss)}")


if __name__ == "__main__":
    main()
