#!/usr/bin/env python3
"""对 KeyFlux-CommandInput.exe 施加「抑制八角 keycap」数据 patch。

背景 (详见 docs/CONTRACTS.md §3.11)
------------------------------------
命令框 exe 会对 `a-zA-Z0-9` 这 62 个字符绘制八角形 keycap 外框。绘制判定所用的
字符白名单是 .rdata 里的**常量数据**, 不是代码 —— 因此可以安全地做数据 patch,
而不必 (也不能) 改控制流:

    文件偏移 0x1CCA0  = RVA 0x1DAA0
    内容     62 个 UTF-16LE 字符: abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789
    前邻     "ac e->EndDraw()\\0\\0"     (即 Draw() 的字面量之后)
    后邻     "\\0\\0\\0\\0 dwriteFactory-"

把这 62 个字符**逐字替换为 U+0001 (不可打印控制字符, 保留长度 124 字节)**后,
该白名单不再匹配任何真实输入字符 ⇒ 字母/数字走普通字形路径 ⇒ 命令框内直接显示、
无八角框。长度不变 ⇒ 不移动任何后续数据, 段表/校验和均不受影响。

⚠ 为什么必须有这个脚本
--------------------
`make sync-out` 的 robocopy 白名单含 `'*.exe'`, 会用**仓库侧未 patch** 的 exe
覆盖部署树 ⇒ patch 是部署树本地事实, **每次 sync-out 之后都会失效**。
本脚本幂等, 可反复执行; 已接入 Makefile `sync-out` 末尾, 自动在 robocopy 之后重施。

用法
----
    python tools/patch_command_input.py <exe路径>
    python tools/patch_command_input.py <exe路径> --check     # 只报告状态, 不写
    python tools/patch_command_input.py <exe路径> --revert    # 还原成官方原版白名单

退出码: 0 成功/幂等跳过, 1 前置校验失败 (文件不符预期, 绝不盲写)。
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# ---- patch 目标 (单一真源; 与 docs/CONTRACTS.md §3.11 保持一致) ----
OFFSET = 0x1CCA0
KEYCAP_WHITELIST = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
N_CHARS = len(KEYCAP_WHITELIST)  # 62
PATCH_CHAR = "\u0001"
PATCHED_BYTES = (PATCH_CHAR * N_CHARS).encode("utf-16-le")

# 邻域哨兵: 用于确认我们打的是正确版本 exe 的正确位置。
# 一律用**显式 hex 字节**表达 —— 转义 NUL 在 Python 源码里极易数错字节数。
#   0x1CC80 起 16 B = 'a','c','e','-','>','E','n','d'   (UTF-16LE, 源自 "ace->EndDraw()")
#   0x1CD1C 起 16 B = NUL,NUL,'d','w','r','i','t','e'   (UTF-16LE, 源自 "\0\0dwriteFactory-")
SENTINEL_BEFORE_OFF = 0x1CC80
SENTINEL_BEFORE = bytes.fromhex("6100630065002d003e0045006e0064")
SENTINEL_AFTER_OFF = 0x1CD1C
SENTINEL_AFTER = bytes.fromhex("000000006400770072006900740065")


def classify(blob: bytes) -> str:
    """返回 'patched' / 'original' / 'unknown'。"""
    seg = blob[OFFSET : OFFSET + N_CHARS * 2]
    if seg == PATCHED_BYTES:
        return "patched"
    if seg == KEYCAP_WHITELIST.encode("utf-16-le"):
        return "original"
    return "unknown"


def verify_sentinels(blob: bytes) -> str | None:
    """校验 patch 点前后的哨兵字节; 不符则返回错误描述。"""
    before = blob[SENTINEL_BEFORE_OFF : SENTINEL_BEFORE_OFF + len(SENTINEL_BEFORE)]
    after = blob[SENTINEL_AFTER_OFF : SENTINEL_AFTER_OFF + len(SENTINEL_AFTER)]
    if before != SENTINEL_BEFORE:
        return (
            f"前置哨兵不符: 期望 {SENTINEL_BEFORE.hex()}, 实得 {before.hex()} "
            f"(偏移 {SENTINEL_BEFORE_OFF:#x}) —— exe 版本可能已变更, 拒绝写入"
        )
    if after != SENTINEL_AFTER:
        return (
            f"后置哨兵不符: 期望 {SENTINEL_AFTER.hex()}, 实得 {after.hex()} "
            f"(偏移 {SENTINEL_AFTER_OFF:#x}) —— exe 版本可能已变更, 拒绝写入"
        )
    return None


def main() -> int:
    ap = argparse.ArgumentParser(description="KeyFlux 命令框八角 keycap 抑制 patch")
    ap.add_argument("exe", help="KeyFlux-CommandInput.exe 路径")
    ap.add_argument("--check", action="store_true", help="只报告状态, 不写文件")
    ap.add_argument("--revert", action="store_true", help="还原为官方原版白名单")
    args = ap.parse_args()

    p = Path(args.exe)
    if not p.is_file():
        print(f"[FAIL] 文件不存在: {p}")
        return 1

    blob = bytearray(p.read_bytes())
    if len(blob) < OFFSET + N_CHARS * 2:
        print(f"[FAIL] 文件过小 ({len(blob)} B), 不可能是目标 exe")
        return 1

    err = verify_sentinels(blob)
    if err:
        print(f"[FAIL] {err}")
        return 1

    state = classify(blob)
    print(f"[info] {p}")
    print(f"[info] 大小 {len(blob)} B, 偏移 {OFFSET:#x} 状态 = {state}")

    if args.check:
        return 0

    want = "original" if args.revert else "patched"
    label = "还原" if args.revert else "施加"

    if state == want:
        print(f"[OK] 已是目标状态 ({want}), 幂等跳过")
        return 0

    if state == "unknown":
        seg = bytes(blob[OFFSET : OFFSET + N_CHARS * 2])
        print(f"[FAIL] 白名单区域内容不符预期, 拒绝写入 (前 32 字节: {seg[:32].hex()})")
        return 1

    payload = (
        KEYCAP_WHITELIST.encode("utf-16-le") if args.revert else PATCHED_BYTES
    )
    blob[OFFSET : OFFSET + N_CHARS * 2] = payload

    # 回读复核: 写盘后重新读文件确认落盘字节正确。
    p.write_bytes(bytes(blob))
    after_state = classify(bytearray(p.read_bytes()))
    if after_state != want:
        print(f"[FAIL] 回读复核失败: 期望 {want}, 实得 {after_state}")
        return 1

    print(f"[OK] {label}成功, 回读复核通过 ({want}); 长度不变 {len(blob)} B")
    return 0


if __name__ == "__main__":
    sys.exit(main())
