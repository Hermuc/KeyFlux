#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""文本特征双端一致性对账 (Go 真源 vs AHK 运行时)。

背景
----
内置文本特征 (url/path/magnet/bilibili/plain) 的命中语义由两份**独立实现**承载:
  - 生成端/校验端: Go  `config-server/internal/behaviors/textfeatures.go`
  - 运行时:        AHK `bin/lib/rules/SelectedAction.ahk`
两份真源之间历来靠"注释里写一句必须一致"约束 (SelectedAction.ahk 曾声称由
`testdata/match_ops.json` 守护, 但该向量只有 Go 侧消费) —— 属于**无强制力的契约**。
本工具把这条契约变成可执行断言, 两层校验:

  1. 静态对账: 解析两侧注册表, 逐项比对 value / named / ignoreCase / pattern 与**顺序**,
     并断言顺序与共享向量 testdata/text_types.json 的 `types` 完全一致。
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

# ---------------------------------------------------------------- 源文件定位

GO_SRC = os.path.join("config-server", "internal", "behaviors", "textfeatures.go")
AHK_SRC = os.path.join("bin", "lib", "rules", "SelectedAction.ahk")
VECTOR = os.path.join("config-server", "internal", "script", "testdata", "text_types.json")

# ---------------------------------------------------------------- 两侧注册表解析
# Go 具名项:  {Value: "url", Label: "链接", Named: true, IgnoreCase: true, Pattern: `^(https?|ftp)://`},
# Go 兜底项:  {Value: "plain", Label: "纯文本", Fallback: true},
GO_ROW = re.compile(
    r'^[ \t]*\{[ \t]*Value:[ \t]*"(?P<value>[a-z][a-z0-9_]*)"[ \t]*,[ \t]*'
    r'Label:[ \t]*"(?P<label>[^"]*)"[ \t]*,(?P<rest>.*)\}[ \t]*,[ \t]*$'
)
GO_PATTERN = re.compile(r'Pattern:[ \t]*`(?P<pattern>[^`]*)`')
GO_IGNORECASE = re.compile(r'IgnoreCase:[ \t]*(?P<flag>true|false)')

# AHK:  {value: "url", named: true, ignoreCase: true, pattern: "^(https?|ftp)://"},
AHK_ROW = re.compile(
    r'^[ \t]*\{[ \t]*value:[ \t]*"(?P<value>[a-z][a-z0-9_]*)"[ \t]*,[ \t]*'
    r'named:[ \t]*(?P<named>true|false)[ \t]*,[ \t]*'
    r'ignoreCase:[ \t]*(?P<ignoreCase>true|false)[ \t]*,[ \t]*'
    r'pattern:[ \t]*"(?P<pattern>[^"]*)"[ \t]*\}[ \t]*,[ \t]*$'
)

COMPARE_KEYS = ("value", "named", "ignoreCase", "pattern")


def read_text(path: str) -> str:
    with io.open(path, "r", encoding="utf-8-sig") as fh:
        return fh.read()


def parse_go_registry(repo: str):
    src = read_text(os.path.join(repo, GO_SRC))
    rows = []
    for line in src.splitlines():
        m = GO_ROW.match(line)
        if not m:
            continue
        rest = m.group("rest")
        pm = GO_PATTERN.search(rest)
        im = GO_IGNORECASE.search(rest)
        rows.append(
            {
                "value": m.group("value"),
                "label": m.group("label"),
                "named": pm is not None,
                "ignoreCase": bool(im and im.group("flag") == "true"),
                "pattern": pm.group("pattern") if pm else "",
            }
        )
    return rows


def extract_ahk_func(src: str, name: str):
    """逐字提取 `name(...) {` 到顶格 `}` 的函数体 (含首尾行)。未找到返回 None。"""
    m = re.search(r"(?m)^%s\(.*?\)[ \t]*\{.*?^\}" % re.escape(name), src, re.S)
    return m.group(0) if m else None


def parse_ahk_registry(repo: str):
    src = read_text(os.path.join(repo, AHK_SRC))
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
                "label": "",
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
        "; 下列函数体从 bin/lib/rules/SelectedAction.ahk **逐字提取** (非手抄),",
        "; 目的 = 让 PCRE2 侧行为与 Go/RE2 侧 (testdata/text_types.json) 逐条对齐。",
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
    for rel in (GO_SRC, AHK_SRC, VECTOR):
        if not os.path.isfile(os.path.join(repo, rel)):
            print("[FAIL] 缺少文件: %s" % rel)
            return 2

    doc = json.loads(read_text(os.path.join(repo, VECTOR)))
    types = doc["types"]
    cases = doc["cases"]

    go_rows = parse_go_registry(repo)
    ahk_rows = parse_ahk_registry(repo)
    failures = []

    # ---- 1. 注册表静态对账 ----
    if not go_rows:
        failures.append("未能从 %s 解析出任何注册表项" % GO_SRC)
    if [r["value"] for r in go_rows] != list(types):
        failures.append("Go 注册表取值/顺序 %s ≠ 向量 types %s"
                        % ([r["value"] for r in go_rows], list(types)))
    if ahk_rows is None:
        failures.append("AHK 源中未找到 TextFeatureSpecs() 注册表")
    else:
        if len(ahk_rows) != len(go_rows):
            failures.append("AHK 注册表 %d 项 ≠ Go %d 项" % (len(ahk_rows), len(go_rows)))
        for i, (g, a) in enumerate(zip(go_rows, ahk_rows)):
            for key in COMPARE_KEYS:
                if g[key] != a[key]:
                    failures.append(
                        "注册表第 %d 项字段 %s 不一致: Go=%r AHK=%r (value=%s)"
                        % (i + 1, key, g[key], a[key], g["value"])
                    )

    # 结构性不变量: 兜底项唯一且居末 (plain 的"其余都不命中"语义依赖它)
    if go_rows:
        fallbacks = [r["value"] for r in go_rows if not r["named"]]
        if fallbacks != [types[-1]]:
            failures.append("兜底特征必须唯一且居末, 实际 = %s (types 末位 %s)"
                            % (fallbacks, types[-1]))

    if failures:
        for f in failures:
            print("[FAIL] " + f)
        return 1
    print("[OK] 注册表静态对账: %d 项, 顺序 %s" % (len(go_rows), " / ".join(types)))

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
        src = read_text(os.path.join(repo, AHK_SRC))
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
