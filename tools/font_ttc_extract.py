#!/usr/bin/env python3
"""从 .ttc/.otc 字体集合中抽取**单个 face**, 另存为独立 .ttf。

为什么需要它: 命令框 exe 只知道一个落点 (`bin/font/font.ttf`), 且 DirectWrite 私有字体
集合会加载**整个文件** —— 把 48 个 face 的 83MB `.ttc` 直接丢进去既超体积上限, 又会
只认首 face (常见是 CL/JP 等非简体中文变体)。更纱黑体 (Sarasa Gothic) 正是这种
「6 风格 × 8 语言区 = 48 face」的集合结构。

用法:
  # 列出集合里全部 face (族名/字重), 便于挑 idx
  python3 tools/font_ttc_extract.py Sarasa-ExtraLight.ttc --list

  # 按族名关键字抽取 (推荐, 比记 idx 稳)
  python3 tools/font_ttc_extract.py Sarasa-ExtraLight.ttc \\
      --family "Sarasa Gothic SC" --out SarasaGothicSC-XLight.ttf

  # 按 face 序号抽取
  python3 tools/font_ttc_extract.py Sarasa-ExtraLight.ttc --index 1 --out x.ttf

另存后建议再用 tools/font_weight_meta.py 把 usWeightClass 对齐到 700 (命令框 exe 请求
BOLD(700); 非 700 会触发 DirectWrite 合成加粗 ⇒ 中文糊)。

依赖: fontTools。
"""

import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from fontTools.ttLib import TTCollection  # noqa: E402

# 集合成员表: .ttc/.otc 都是 sfnt 打包 (fontTools 统一用 TTCollection 读写)
_COLLECTION_EXTS = {".ttc", ".otc"}


def _font_kind(f):
    """返回 'glyf' / 'CFF' / '?' —— 命令框 exe 只接受 glyf。"""
    tags = set(f.reader.keys()) if hasattr(f, "reader") else set(f.keys())
    if "glyf" in tags:
        return "glyf"
    if "CFF " in tags or "CFF2" in tags:
        return "CFF"
    return "?"


def _face_label(f, idx):
    def _name(nid):
        try:
            return f["name"].getDebugName(nid) or "?"
        except Exception:
            return "?"

    fam = _name(1)
    sub = _name(2)
    try:
        w = f["OS/2"].usWeightClass
    except Exception:
        w = -1
    return f"[{idx:3d}] {fam} | {sub} | wght={w} | {_font_kind(f)}"


def _load_collection(path):
    if os.path.splitext(path)[1].lower() not in _COLLECTION_EXTS:
        # 仍给一次机会 —— 有些 .ttf 其实是单 face 集合 (ttcf 头)
        pass
    return TTCollection(path, lazy=False)


def _pick_by_family(coll, keyword):
    """按族名子串匹配 (大小写不敏感), 返回全部命中的 idx 列表。"""
    kw = keyword.lower()
    hits = []
    for i, f in enumerate(coll.fonts):
        try:
            fam = (f["name"].getDebugName(1) or "").lower()
        except Exception:
            continue
        if kw in fam:
            hits.append(i)
    return hits


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("src", help=".ttc/.otc 字体集合路径")
    ap.add_argument("--list", action="store_true", help="只列出全部 face, 不抽取")
    ap.add_argument("--index", type=int, default=None, help="抽取的 face 序号 (从 0 起)")
    ap.add_argument("--family", default=None, help="按族名子串抽取 (大小写不敏感); 命中多个时报错")
    ap.add_argument("--out", default=None, help="输出 .ttf 路径")
    a = ap.parse_args()

    if not os.path.isfile(a.src):
        raise SystemExit(f"源文件不存在: {a.src}")

    coll = _load_collection(a.src)

    if a.list:
        print(f"{a.src}  共 {len(coll.fonts)} 个 face:")
        for i, f in enumerate(coll.fonts):
            print("  " + _face_label(f, i))
        return

    if a.index is None and a.family is None:
        raise SystemExit("需要 --index 或 --family 指定要抽的 face (或用 --list 先看看)")
    if a.index is not None and a.family is not None:
        raise SystemExit("--index 与 --family 只能给一个")
    if not a.out:
        raise SystemExit("抽取时必须给 --out")

    if a.family is not None:
        hits = _pick_by_family(coll, a.family)
        if not hits:
            raise SystemExit(f"没有族名含 \"{a.family}\" 的 face; 用 --list 查看全部")
        if len(hits) > 1:
            raise SystemExit(
                f"族名含 \"{a.family}\" 的 face 有 {len(hits)} 个 ({hits}), 请用 --index 精确指定")
        idx = hits[0]
    else:
        idx = a.index

    if not (0 <= idx < len(coll.fonts)):
        raise SystemExit(f"--index {idx} 越界 (0..{len(coll.fonts) - 1})")

    f = coll.fonts[idx]
    kind = _font_kind(f)
    print(f"选中 face: {_face_label(f, idx)}")

    # 集合成员常带 VORG (CFF 专用) 等无用表; glyf 字体可直接另存
    f.save(a.out)
    size = os.path.getsize(a.out)
    print(f"saved {a.out} ({size:,} B)  轮廓={kind}")

    # 回读自检 (与其它字体工具同口径)
    from fontTools.ttLib import TTFont
    g = TTFont(a.out, lazy=True)
    gt = set(g.reader.keys())
    print(f"reload OK; 表数={len(gt)}; glyf={'glyf' in gt}; "
          f"字形数={len(g.getGlyphOrder())}; "
          f"usWeightClass={g['OS/2'].usWeightClass}")
    if kind != "glyf":
        print("⚠ 该 face 不是 glyf 轮廓 —— 命令框 exe 只接受 glyf, "
              "请改用 tools/font_otf2ttf.py 转换后再用。", file=sys.stderr)


if __name__ == "__main__":
    main()
