#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""文本特征双端一致性对账 (Rust 面板镜像 vs AHK 运行时, 共享向量钉死行为)。

背景
----
内置文本特征 (url/path/magnet/bilibili/plain) 的**取值与顺序**由两处独立声明承载:
  - 面板镜像:  Rust `config-ui-reactor/src/services/selected_action.rs` :: TEXT_TYPES
    (真源随 2026-10-06 Go 后端退役迁移至此 —— 原 Go 注册表
    `config-server/internal/behaviors/textfeatures.go` 的 value/顺序由本镜像继承);
  - 运行时:    AHK `bin/lib/rules/SelectedAction.ahk` 及其子模块 `SelectedAction/*.ahk`
    (TextFeatureSpecs 注册表 +
    MatchTextType 命中逻辑, 含正则/大小写开关 —— Go 退役后正则字面量的**唯一来源**)。
两处之间靠共享向量 `testdata/text_types.json` 钉死 (2026-10-06 自
config-server/internal/script/testdata/ 迁出; 同日删除仅 Go 消费的 match_ops.json
—— 其契约早已被判定为无强制力, 见 git 历史)。
本工具把这条契约变成可执行断言, 两层校验:

  1. 静态对账: 解析两侧注册表, 逐项比对 **value 与顺序**,
     并断言与共享向量 text_types.json 的 `types` 完全一致; 兜底特征唯一且居末。
     (named/ignoreCase/pattern 在 Go 退役后只剩 AHK 单一来源, 无对账意义 ——
     其行为正确性由第 2 层向量对账兜底。)
  2. 运行时对账: 从 AHK 源**逐字提取**函数体 (不手抄, 杜绝探针与产品代码漂移),
     按共享向量的用例逐 (用例 × 特征) 求值, 比对 expectTypes 全集。

判据是可执行断言而非注释 ⇒ 任何一侧改了语义而另一侧没跟上, 会在本地 make / CI 中变红。

用法
----
    python3 tools/texttype_conformance.py [--repo .] [--ahk bin/AutoHotkey64.exe] [--verbose]
    python3 tools/texttype_conformance.py --static-only     # 只做注册表静态对账, 不跑解释器

退出码: 0 = 全过; 1 = 语义/注册表失配; 2 = 基础设施错误 (文件缺失 / 解释器不可用)。
"""

from __future__ import annotations

import argparse
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

# CI 的 Windows runner 上 stdout 默认继承 locale 编码 (cp1252), 打印中文即
# UnicodeEncodeError —— 本脚本所有诊断文案都是中文, 这里统一强制 UTF-8
# (reconfigure 幂等; errors=replace 兜底, 保证闸门永不因编码问题误红)。
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        _stream.reconfigure(encoding="utf-8", errors="replace")

# ---------------------------------------------------------------- 源文件定位

RUST_SRC = os.path.join("config-ui-reactor", "src", "services", "selected_action.rs")
AHK_SRC = os.path.join("bin", "lib", "rules", "SelectedAction.ahk")
# 🔴 2026-10-08（模块化审查 §3.3）：SelectedAction.ahk 已拆为「门面 + 同目录子模块」。
#   门面只用**嵌套 #Include** 引入 `SelectedAction/*.ahk`（生成物 KeyFlux.ahk 里仍只有一行
#   `#Include lib/rules/SelectedAction.ahk`，故 parity `ahk` 基线不受影响）。
#   本工具因此按「门面 + 子目录」**拼成一份虚拟源**再逐字抽取 —— 以后再拆/挪子文件时，
#   只要还在该目录内就无需再改本工具（这正是当初硬编码单文件路径的脆弱点）。
#   拼接顺序无关紧要：抽取按函数名正则定位，不做位置假设。
AHK_SUBDIR = os.path.join("bin", "lib", "rules", "SelectedAction")
VECTOR = os.path.join("testdata", "text_types.json")

# ---------------------------------------------------------------- 两侧注册表解析
# Rust 镜像:  pub const TEXT_TYPES: [(&str, &str); 5] = [
#                 ("url", "1059"), ... ("plain", "1062"),
#             ];
#   (第二个元素是 i18n 键, 不参与对账 —— 标签文案不属于本契约。)
# AHK:  {value: "url", named: true, ignoreCase: true, pattern: "^(https?|ftp)://"},
AHK_ROW = re.compile(
    r'^[ \t]*\{[ \t]*value:[ \t]*"(?P<value>[a-z][a-z0-9_]*)"[ \t]*,[ \t]*'
    r'named:[ \t]*(?P<named>true|false)[ \t]*,[ \t]*'
    r'ignoreCase:[ \t]*(?P<ignoreCase>true|false)[ \t]*,[ \t]*'
    r'pattern:[ \t]*"(?P<pattern>[^"]*)"[ \t]*\}[ \t]*,[ \t]*$'
)


def read_text(path: str) -> str:
    with io.open(path, "r", encoding="utf-8-sig") as fh:
        return fh.read()


def ahk_source_files(repo: str):
    """AHK 侧契约源文件清单 = 门面 + 子目录内全部 .ahk（按名排序）。"""
    out = [os.path.join(repo, AHK_SRC)]
    sub = os.path.join(repo, AHK_SUBDIR)
    if os.path.isdir(sub):
        out += [os.path.join(sub, n) for n in sorted(os.listdir(sub)) if n.endswith(".ahk")]
    return out


def read_ahk_virtual_source(repo: str) -> str:
    """把门面 + 子模块拼成一份虚拟源（抽取按函数名定位，故顺序无关）。"""
    return "\n".join(read_text(p) for p in ahk_source_files(repo))


def parse_rust_registry(repo: str):
    """解析 services/selected_action.rs 的 TEXT_TYPES 常量 → value 有序列表。"""
    src = read_text(os.path.join(repo, RUST_SRC))
    m = re.search(
        r"TEXT_TYPES:\s*\[\(&str,\s*&str\);\s*\d+\]\s*=\s*\[(.*?)\];",
        src,
        re.S,
    )
    if not m:
        return None
    return [
        mm.group("value")
        for mm in re.finditer(r'\("(?P<value>[a-z][a-z0-9_]*)",\s*"[^"]*"\)', m.group(1))
    ]


def extract_ahk_func(src: str, name: str):
    """逐字提取 `name(...) {` 到顶格 `}` 的函数体 (含首尾行)。未找到返回 None。"""
    m = re.search(r"(?m)^%s\(.*?\)[ 	]*\{.*?^\}" % re.escape(name), src, re.S)
    return m.group(0) if m else None


def parse_ahk_registry(repo: str):
    src = read_ahk_virtual_source(repo)
    body = extract_ahk_func(src, "TextFeatureSpecs")
    if body is None:
        return None
    rows = []
    for line in body.splitlines():
        m = AHK_ROW.match(line)
        if not m:
            continue
        rows.append(
            {
                "value": m.group("value"),
                "named": m.group("named") == "true",
                "ignoreCase": m.group("ignoreCase") == "true",
                "pattern": m.group("pattern"),
            }
        )
    return rows


# ---------------------------------------------------------------- 探针生成


def ahk_escape(literal: str) -> str:
    """把 Python 字符串转成 AHK 双引号字面量内容 (AHK 只认反引号转义, 不认反斜杠)。"""
    out = []
    for ch in literal:
        if ch == "`":
            out.append("``")
        elif ch == '"':
            out.append('`"')
        elif ch == "\n":
            out.append("`n")
        elif ch == "\r":
            out.append("`r")
        elif ch == "\t":
            out.append("`t")
        else:
            out.append(ch)
    return "".join(out)


def build_probe(src: str, types, cases, result_path: str) -> str:
    funcs = []
    for name in ("TextFeatureSpecs", "TextFeatureHit", "MatchTextType"):
        body = extract_ahk_func(src, name)
        if body is None:
            if name == "TextFeatureHit":
                continue  # 兼容尚未引入该辅助函数的旧版实现
            raise RuntimeError("AHK 源中未找到函数 %s" % name)
        funcs.append(body)

    probe = [
        "#Requires AutoHotkey v2.0",
        "; !!! 本文件由 tools/texttype_conformance.py 生成, 请勿手工编辑 !!!",
        "; 下列函数体从 bin/lib/rules/SelectedAction.ahk + SelectedAction/*.ahk **逐字提取** (非手抄),",
        "; 目的 = AHK PCRE2 命中行为与共享向量 (testdata/text_types.json) 逐条对齐。",
        "",
    ]
    probe.append("\n".join(funcs))
    probe += [
        "",
        'p := "%s"' % result_path.replace("\\", "/"),
        'buf := ""',
        "try {",
    ]
    for ci, case in enumerate(cases):
        content = ahk_escape(case["content"])
        for ti, t in enumerate(types):
            probe.append(
                '\tbuf .= "%d.%d=" . (MatchTextType("%s", "%s") ? "1" : "0") . "`n"'
                % (ci, ti, t, content)
            )
    probe += [
        "} catch as e {",
        '\tbuf .= "ERR:" . e.Message . "`n"',
        "}",
        "try FileDelete(p)",
        'try FileAppend(buf, p, "UTF-8")',
        "ExitApp(0)",
        "",
    ]
    return "\n".join(probe)


# ---------------------------------------------------------------- 主流程


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=".")
    ap.add_argument("--ahk", default=os.path.join("bin", "AutoHotkey64.exe"))
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--static-only", action="store_true", help="只做注册表静态对账, 不跑解释器")
    args = ap.parse_args()

    repo = os.path.abspath(args.repo)
    for rel in (RUST_SRC, VECTOR):
        if not os.path.isfile(os.path.join(repo, rel)):
            print("[FAIL] 缺少文件: %s" % rel)
            return 2
    for src_path in ahk_source_files(repo):
        if not os.path.isfile(src_path):
            print("[FAIL] 缺少文件: %s" % os.path.relpath(src_path, repo))
            return 2

    doc = json.loads(read_text(os.path.join(repo, VECTOR)))
    types = doc["types"]
    cases = doc["cases"]

    rust_values = parse_rust_registry(repo)
    ahk_rows = parse_ahk_registry(repo)
    failures = []

    # ---- 1. 注册表静态对账 (value + 顺序; 模式/大小写行为由第 2 层向量兜底) ----
    if not rust_values:
        failures.append("未能从 %s 解析出任何注册表项" % RUST_SRC)
    if rust_values != list(types):
        failures.append("Rust 镜像取值/顺序 %s ≠ 向量 types %s"
                        % (rust_values, list(types)))
    if ahk_rows is None:
        failures.append("AHK 源中未找到 TextFeatureSpecs() 注册表")
    else:
        ahk_values = [r["value"] for r in ahk_rows]
        if ahk_values != list(types):
            failures.append("AHK 注册表取值/顺序 %s ≠ 向量 types %s"
                            % (ahk_values, list(types)))

    # 结构性不变量: 兜底项唯一且居末 (plain 的"其余都不命中"语义依赖它)
    if ahk_rows:
        fallbacks = [r["value"] for r in ahk_rows if not r["named"]]
        if fallbacks != [types[-1]]:
            failures.append("兜底特征必须唯一且居末, 实际 = %s (types 末位 %s)"
                            % (fallbacks, types[-1]))

    if failures:
        for f in failures:
            print("[FAIL] " + f)
        return 1
    print("[OK] 注册表静态对账: %d 项, 顺序 %s" % (len(types), " / ".join(types)))

    if args.static_only:
        return 0

    # ---- 2. AHK 运行时对账 ----
    ahk_bin = args.ahk if os.path.isabs(args.ahk) else os.path.join(repo, args.ahk)
    if not os.path.isfile(ahk_bin):
        print("[FAIL] 找不到 AHK 解释器: %s" % ahk_bin)
        return 2

    work = tempfile.mkdtemp(prefix="kf_textfeat_")
    try:
        probe_path = os.path.join(work, "texttype_probe.ahk")
        result_path = os.path.join(work, "texttype_result.tsv")
        src = read_ahk_virtual_source(repo)
        with io.open(probe_path, "w", encoding="utf-8", newline="\r\n") as fh:
            fh.write(build_probe(src, types, cases, result_path))

        try:
            proc = subprocess.run(
                [ahk_bin, "/ErrorStdOut", probe_path], capture_output=True, timeout=120
            )
        except subprocess.TimeoutExpired:
            print("[FAIL] AHK 探针超时 (120s): 疑似运行时错误对话框阻塞")
            return 2
        if proc.returncode != 0:
            err = (proc.stderr or proc.stdout or b"").decode("utf-8", "replace").strip()
            print("[FAIL] AHK 探针退出码 %d: %s" % (proc.returncode, err[:400]))
            return 2
        if not os.path.isfile(result_path):
            print("[FAIL] AHK 探针未产出结果文件 (运行时异常?)")
            return 2

        bits = {}
        for line in read_text(result_path).splitlines():
            if line.startswith("ERR:"):
                print("[FAIL] AHK 运行时错误: " + line)
                return 2
            if "=" not in line:
                continue
            key, _, val = line.partition("=")
            bits[key.strip()] = (val.strip() == "1")

        diffs = []
        for ci, case in enumerate(cases):
            expected = set(case["expectTypes"])
            got = set()
            for ti, t in enumerate(types):
                key = "%d.%d" % (ci, ti)
                if key not in bits:
                    diffs.append("用例#%d 缺结果 (%s)" % (ci + 1, key))
                    continue
                if bits[key]:
                    got.add(t)
            if got != expected:
                diffs.append(
                    "用例#%d content=%r\n    期望命中 %s\n    AHK 命中 %s\n    note=%s"
                    % (ci + 1, case["content"], sorted(expected) or ["<无>"],
                       sorted(got) or ["<无>"], case.get("note", ""))
                )

        if diffs:
            for d in diffs[:20]:
                print("[FAIL] AHK↔向量失配: " + d)
            print("[FAIL] 共 %d 条用例失配" % len(diffs))
            return 1
        print("[OK] AHK 运行时对账: %d 用例 × %d 特征 = %d 次求值全部一致"
              % (len(cases), len(types), len(cases) * len(types)))
        return 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
