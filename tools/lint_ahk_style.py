#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""AHK 侧「风格一致性 + 静默失败面」护栏 (2026-10-08 规范化批次 G)。

为什么需要这个工具
------------------
本仓库的 AHK 侧长期缺一把**可执行的尺子**：Rust 侧有 `cargo fmt` / `clippy`，
而手写的 `.ahk`（引擎 + 插件 + 工具）的风格与「失败是否留痕」只能靠人眼。2026-10-07/10-08 两轮审查
各自"人工整理"过一批（空 catch、BOM、NUL 字节），但**没有护栏** ⇒ 整理完就会回退。

本脚本把若干事实变成计数 + `文件:行`，并用 `--write-baseline` 把当前值冻结成基线：
后续任何改动只要让某类计数**变差**（高于基线）即 exit 1。
判据不是「绝对零」而是「不高于基线」—— 存量债分批偿还（批 L/M/N/O），每批只能往下压。
基线文件：`tools/lint_ahk_style.baseline.json`。

扫描范围
--------
* **文本形态**（`bom_files` / `crlf_files` / `tab_indent_files` / `spelling_drift`）：
  `bin/lib` + `bin` 顶层 + `plugins` + `tools` 顶层 —— 与 `.gitattributes` 声明 `eol=lf`
  的四类**逐一对应**（那些声明的存在理由就是本护栏；此前只扫 `bin/lib`，其余三类的
  声明形同虚设，2026-10-09 补齐）。生成物 `bin/KeyFlux.ahk` 与 parity 冻结基线**排除**。
* **静默失败面**（`try_no_catch` / `catch_no_trace`）与 `log_sinks`：**仅引擎核心 `bin/lib`**
  —— 插件/工具是独立层次，其 best-effort `try` 语义与引擎不同，纳入只会引入无关噪声。

检查项
------
1. `try_no_catch`   —— `try` 之后既无 `catch` 也无 `finally`（= 吞掉 Error 类异常）。
2. `catch_no_trace` —— `catch` 体内没有任何留痕调用（`EngineLogWarn` / `EngineOnError`
   / 自建 `_log` / `LogError`）。「刻意吞掉」与「留痕」并不冲突。
3. `log_sinks`      —— `bin/lib` 内 `FileAppend(..., "logs\\...")` 触及的**不同日志文件名**数。
   规范入口只有一个（`core/Functions.ahk` 的 `engine_error.log`）。
   注：插件**源码**不在 `bin/lib` 下（在仓库根 `plugins/examples/`），故本项即引擎侧口径。
4. `bom_files` / `crlf_files` / `tab_indent_files` —— 文本形态漂移。
   `core/Monitor.ahk` 是 vendored 上游库（头注声明零改动）⇒ 缩进检查豁免。
5. `spelling_drift` —— 同概念拼写分裂（`Cpas` / `Casp` 均为 `Caps` 的字母错位；
   2026-10-08 批 O 已统一，此处保留检查以防回退）。

🔴 四个**必须**处理的形态（首版全踩过，逐个留证）
   ① `try { … } catch { … }` 里 try 的收尾 `}` 与 catch **同行**（`} catch {`）
      —— 只看「下一行是不是 catch」会漏判（把有 catch 的判成静默）；
   ② `catch as e` 独占一行、体在**下一行**（甚至无花括号）
      —— 只看 catch 本行会漏判「有留痕」；
   ③ `try { … } finally { … }`（**不**吞异常，异常照常穿透）
      —— 算作静默失败是假阳性；
   ④ `/** */` 文档块里的 `catch` / `try` 字样
      —— 只剥 `;` 行尾注释会让**注释里的关键词**被当成真代码（假阳性）。
   因此：按**大括号深度**求语句范围（不是找「首个 `}` 行」）；catch 关键字按
   「行内任意位置」判定；先剥**块注释**再判定。

🔴 豁免机制：引擎的**日志设施自身**不能在失败时再调日志（递归风险），退出路径同理。
   这类位置用 `EXEMPT_FUNCS` **按「文件 + 所在函数名」**豁免（不按行号，抗漂移），
   并带**腐烂自检**：豁免项若已不复存在 / 未命中任何 finding，直接 FAIL。

用法
----
    python tools/lint_ahk_style.py                 # 对比基线；变差则 exit 1
    python tools/lint_ahk_style.py --verbose        # 逐条打印 文件:行
    python tools/lint_ahk_style.py --write-baseline  # 用当前值覆盖基线
    python tools/lint_ahk_style.py --report         # 只打印，不与基线比较

⚠️ 字节判定一律用 Python 读文件字节，**不要**改用 Git Bash 的 `wc -c` / `grep -c $'\\r'`
   —— 本项目实测其读数不实。
"""
from __future__ import annotations

import argparse
import glob
import io
import json
import os
import re
import sys

for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        _stream.reconfigure(encoding="utf-8", errors="replace")

ENGINE_ROOT = "bin/lib"
BASELINE = os.path.join("tools", "lint_ahk_style.baseline.json")

# 文本形态检查（bom / crlf / tab / spelling）的 scope —— 与 `.gitattributes` 里声明
# `eol=lf` 的四类**逐一对应**：那些声明的存在理由就是这个护栏，而此前它只扫 bin/lib
# ⇒ 另外三类的声明实际上无人检查（2026-10-09 补齐，消除"承诺了却没执行"的守卫）。
# 键一律用**仓库相对路径**。
TEXT_DIR_SCOPES = ("bin/lib", "plugins")           # 递归
TEXT_FILE_SCOPES = ("bin/*.ahk", "tools/*.ahk")     # 仅顶层（避免扫到 parity 冻结基线）
# 生成物（不入库、按设计 CRLF）与 parity reference 不参与文本形态检查。
TEXT_EXCLUDE = {"bin/KeyFlux.ahk"}

# 静默失败面（try/catch）与日志 sink 只对**引擎核心**求值 —— 插件/工具是独立层次，
# 其 best-effort try 语义与引擎不同（见文件头注「检查项」）。
ENGINE_PREFIX = "bin/lib/"

# vendored 上游库：缩进用 tab（上游原样）。改它 = 破坏 vendor 差异基线
# （先例见 vendor/windows-reactor/PATCHES.md）。
VENDORED = {"bin/lib/core/Monitor.ahk"}

# 「同概念拼写分裂」：键 = 应归零的误拼，值 = 正解（仅用于提示）。
# 首版把 `Casp` 当规范、只查 `Cpas`；实测**两者都是错**——`Caps`(CapsLock 命令框) 的
# 字母错位，且上游 fork 快照(`dbf0aca`)即已两种混用。2026-10-08 批 O 统一为 `Caps`，
# 这里把两个旧拼写**都**登记为误拼（防回退）。
SPELLING_WRONG = {"Cpas": "Caps", "Casp": "Caps"}

# 留痕调用：出现任一即视为「catch 有留痕」。
# `_recordError` 是 PluginManager 的插件错误记录设施（面向插件错误面板/日志），
# 与 `_log` 同性质 —— 首版漏了它 ⇒ 把 2 处有留痕的 catch 判成静默（实测）。
TRACE_CALLS = ("EngineLogWarn(", "EngineOnError(", "LogError(", "_log(", "_recordError(", "QSLogWarn(")

# 静默失败的**文档化豁免**（按「文件 + 所在函数名」，抗行号漂移）。三类理由：
#   ① 日志设施自身 / 进程退出路径：不能再调日志（递归、设施不可用）；
#   ② 上一轮审查**已裁定接受**的「有解释的刻意不记」（理由写在 catch 内的注释里）——
#      本表只是把那个裁定编码化，避免下一轮把它当新问题重新"修"一遍。
EXEMPT_FUNCS = {
    ("bin/lib/core/Functions.ahk", "EngineOnError"): "日志设施自身：兜底函数内不能再调日志",
    ("bin/lib/core/Functions.ahk", "EngineLogWarn"): "日志设施自身：递归风险",
    ("bin/lib/core/Functions.ahk", "KeyFluxExit"): "进程退出路径：日志设施可能已不可用",
    ("bin/lib/core/WindowUtils.ahk", "TryTrayRestoreByNav"):
        "pwsh→powershell 降级；回退仍失败会抛出并由 EngineOnError 统一记录 ⇒ 不重复记"
        "（2026-10-07 审查裁定：把「不记」的理由写进代码，见该处 catch 内注释）",
}

# 解析器自检下限：文本形态 scope（bin/lib 44 + bin 顶层 6 + plugins 17 + tools 3 = 70）
# 实测 70 个 .ahk。低于此值 = 扫描器退化，必须报错而不是"安静地少算"
# （本项目对解析器的硬要求：静默退化 = 假绿）。
MIN_EXPECTED_FILES = 65

FUNC_DEF = re.compile(r"^\s*(?:static\s+)?([A-Za-z_]\w*)\s*\([^)]*\)\s*\{\s*$")


# ---------------------------------------------------------------- 文本处理

def read_bytes(path: str) -> bytes:
    with open(path, "rb") as fh:
        return fh.read()


def build_codes(lines) -> list:
    """把源行一次性转成「纯代码行」：剥掉 `;` 行尾注释与 `/* ... */` 块注释。

    字符串内的 `;` / `/*` 不是注释（AHK 转义符是反引号，不是反斜杠）。
    块注释必须处理：见文件头注 ④。
    """
    out = []
    in_block = False
    for line in lines:
        res = []
        i, in_str = 0, False
        while i < len(line):
            ch = line[i]
            if in_block:
                if ch == "*" and i + 1 < len(line) and line[i + 1] == "/":
                    in_block = False
                    i += 2
                    continue
                i += 1
                continue
            if in_str:
                res.append(ch)
                if ch == "`":
                    i += 1
                    if i < len(line):
                        res.append(line[i])
                        i += 1
                    continue
                if ch == '"':
                    in_str = False
                i += 1
                continue
            if ch == '"':
                in_str = True
                res.append(ch)
                i += 1
                continue
            if ch == ";":
                break
            if ch == "/" and i + 1 < len(line) and line[i + 1] == "*":
                in_block = True
                i += 2
                continue
            res.append(ch)
            i += 1
        out.append("".join(res))
    return out


def indent_of(line: str) -> int:
    return len(line) - len(line.lstrip(" \t"))


def next_code_line(codes, start: int):
    for k in range(start, len(codes)):
        body = codes[k].strip()
        if body:
            return k, body
    return None, ""


def scan_depth(seg: str, depth: int):
    """按大括号扫描一行；返回 (扫描后深度, 深度首次归零之后的余文 | None)。"""
    rem = None
    for idx, ch in enumerate(seg):
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0 and rem is None:
                rem = seg[idx + 1:]
    return depth, rem


def analyze_try(codes, start: int):
    """分析 `try` 语句：返回 (结束行索引, 是否有 catch, 是否有 finally)。

    见文件头注 ①②③：`} catch {` / `} finally {` 必须处理。
    """
    seg0 = codes[start]
    body = seg0.strip()[3:].strip()                 # 'try' 之后
    if body and not body.endswith("{"):
        # 单行 try：catch 可能在同行（罕见）或下一代码行
        if re.search(r"\bcatch\b", seg0):
            return start, True, False
        if re.search(r"\bfinally\b", seg0):
            return start, False, True
        _, nxt = next_code_line(codes, start + 1)
        return start, bool(re.match(r"catch\b", nxt)), bool(re.match(r"finally\b", nxt))

    pos = seg0.find("{")
    depth = 0
    for k in range(start, len(codes)):
        s = codes[k]
        if k == start:
            s = s[pos:]
        depth, rem = scan_depth(s, depth)
        if rem is not None:
            if re.search(r"\bcatch\b", rem):
                return k, True, False
            if re.search(r"\bfinally\b", rem):
                return k, False, True
            _, nxt = next_code_line(codes, k + 1)
            return k, bool(re.match(r"catch\b", nxt)), bool(re.match(r"finally\b", nxt))
    return len(codes) - 1, False, False


def catch_body(codes, i: int):
    """取 `catch` 语句体（含 catch 行）：返回 (起, 止, 文本)。

    catch 关键字可在行内任意位置（`} catch {` 是主流写法，见头注 ①）。
    """
    seg = codes[i]
    m = re.search(r"\bcatch\b", seg)
    after = seg[m.end():].strip()
    if after.endswith("{"):
        pos = seg.find("{", m.end())
        depth = 0
        for k in range(i, len(codes)):
            s = codes[k]
            if k == i:
                s = s[pos:]
            depth += s.count("{")
            depth -= s.count("}")
            if depth <= 0:
                return i, k, "\n".join(codes[i:k + 1])
        return i, len(codes) - 1, "\n".join(codes[i:])
    # 无花括号体：收拢后续更深缩进的行（`catch as e` + 下一行 = 最常见写法，见头注 ②）
    base = indent_of(codes[i])
    body = [codes[i]]
    k = i + 1
    while k < len(codes):
        if not codes[k].strip():
            k += 1
            continue
        if indent_of(codes[k]) <= base:
            break
        body.append(codes[k])
        k += 1
    return i, max(i, k - 1), "\n".join(body)


def function_spans(codes):
    """返回 {行索引: 函数名}（供豁免判定；按大括号深度求函数范围）。"""
    spans = {}
    i = 0
    while i < len(codes):
        mf = FUNC_DEF.match(codes[i])
        if mf:
            pos = codes[i].find("{")
            depth = 0
            end = len(codes) - 1
            for k in range(i, len(codes)):
                s = codes[k]
                if k == i:
                    s = s[pos:]
                depth, rem = scan_depth(s, depth)
                if rem is not None:
                    end = k
                    break
            for k in range(i, end + 1):
                spans[k] = mf.group(1)
            i = end + 1
            continue
        i += 1
    return spans


# ---------------------------------------------------------------- 扫描

def scan_file(path: str, rel: str, f: dict, verbose: bool, is_engine: bool):
    raw = read_bytes(path)
    if raw.startswith(b"\xef\xbb\xbf"):
        f["bom_files"].append(rel)
    text = raw.decode("utf-8-sig", errors="replace")
    if "\r\n" in text:
        f["crlf_files"].append(rel)
    lines = text.split("\n")
    codes = build_codes(lines)

    if rel not in VENDORED and any(ln.startswith("\t") for ln in lines if ln.strip()):
        f["tab_indent_files"].append(rel)

    # 以下两项只对引擎核心（bin/lib）求值；插件/工具脚本是独立层次（见常量区注释）。
    if not is_engine:
        return

    for idx, seg in enumerate(codes, 1):
        if "FileAppend(" in seg and '"logs\\' in seg:
            m = re.search(r'"logs\\([^"]+)"', seg)
            if m:
                f["log_sinks"].add(m.group(1))
                if verbose:
                    print("  [sink] %s:%d -> logs\\%s" % (rel, idx, m.group(1)))

    spans = function_spans(codes)
    for i, seg_raw in enumerate(codes):
        seg = seg_raw.strip()
        if re.match(r"^try\b", seg):
            _end, has_catch, has_finally = analyze_try(codes, i)
            if not has_catch and not has_finally:
                key = (rel, spans.get(i))
                if key in EXEMPT_FUNCS:
                    f["try_exempt"].append("%s:%d (%s)" % (rel, i + 1, spans.get(i)))
                    f["_exempt_hits"].add(key)
                else:
                    f["try_no_catch"].append("%s:%d" % (rel, i + 1))
                    if verbose:
                        print("  [try-no-catch] %s:%d  %s" % (rel, i + 1, seg[:70]))
        elif re.search(r"\bcatch\b", seg_raw):
            _s, _e, body = catch_body(codes, i)
            if not any(tc in body for tc in TRACE_CALLS):
                f["catch_no_trace"].append("%s:%d" % (rel, i + 1))
                if verbose:
                    print("  [catch-no-trace] %s:%d  %s"
                          % (rel, i + 1, " ".join(body.split())[:70]))


def scan_naming(files: dict, f: dict, verbose: bool):
    for wrong, canon in SPELLING_WRONG.items():
        hits = []
        for rel, path in files.items():
            text = read_bytes(path).decode("utf-8-sig", errors="replace")
            for idx, ln in enumerate(build_codes(text.split("\n")), 1):
                if wrong in ln:
                    hits.append("%s:%d" % (rel, idx))
        f["spelling_drift"][wrong] = hits
        if verbose and hits:
            print("  [spelling] %r (应为 %r): %s" % (wrong, canon, ", ".join(hits[:8])))


def discover(repo: str) -> dict:
    """文本形态 scope 下的全部 .ahk（键 = 仓库相对路径）。"""
    files = {}

    def add(full: str):
        rel = os.path.relpath(full, repo).replace(os.sep, "/")
        if rel in TEXT_EXCLUDE:
            return
        files[rel] = full

    for scope in TEXT_DIR_SCOPES:
        for dirpath, _dirs, names in os.walk(os.path.join(repo, scope)):
            for name in sorted(names):
                if name.endswith(".ahk"):
                    add(os.path.join(dirpath, name))
    for pattern in TEXT_FILE_SCOPES:
        for full in sorted(glob.glob(os.path.join(repo, pattern))):
            add(full)
    return files


def scan(repo: str, verbose: bool) -> dict:
    files = discover(repo)

    f = {
        "try_no_catch": [], "catch_no_trace": [], "try_exempt": [],
        "bom_files": [], "crlf_files": [], "tab_indent_files": [],
        "log_sinks": set(), "spelling_drift": {}, "_files_scanned": len(files),
        "_exempt_hits": set(),
    }
    for rel, path in sorted(files.items()):
        scan_file(path, rel, f, verbose, rel.startswith(ENGINE_PREFIX))
    scan_naming(files, f, verbose)
    return f


COUNTER_KEYS = ("try_no_catch", "catch_no_trace", "log_sinks", "bom_files",
                "crlf_files", "tab_indent_files", "spelling_drift_hits")


def to_counters(f: dict) -> dict:
    return {
        "files_scanned": f["_files_scanned"],
        "try_no_catch": len(f["try_no_catch"]),
        "catch_no_trace": len(f["catch_no_trace"]),
        "log_sinks": len(f["log_sinks"]),
        "bom_files": len(f["bom_files"]),
        "crlf_files": len(f["crlf_files"]),
        "tab_indent_files": len(f["tab_indent_files"]),
        "spelling_drift_hits": sum(len(v) for v in f["spelling_drift"].values()),
    }


# ---------------------------------------------------------------- 主流程

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo", default=".")
    ap.add_argument("--verbose", action="store_true", help="逐条打印 文件:行")
    ap.add_argument("--report", action="store_true", help="只打印，不与基线比较")
    ap.add_argument("--write-baseline", action="store_true", help="用当前值覆盖基线")
    args = ap.parse_args()

    repo = os.path.abspath(args.repo)
    if not os.path.isdir(os.path.join(repo, ENGINE_ROOT)):
        print("[FAIL] 找不到 %s（--repo 指错了？）" % ENGINE_ROOT)
        return 2

    f = scan(repo, args.verbose)
    cur = to_counters(f)

    # 豁免清单腐烂自检：声明了豁免却一处未命中 ⇒ 函数被改名/删除，必须同步
    stale = sorted(set(EXEMPT_FUNCS) - f["_exempt_hits"])
    if stale:
        print("[FAIL] 豁免清单已腐烂（%d 项未命中任何 finding）—— 请同步 EXEMPT_FUNCS：" % len(stale))
        for key in stale:
            print("    %s :: %s" % (key, EXEMPT_FUNCS[key]))
        return 2

    if cur["files_scanned"] < MIN_EXPECTED_FILES:
        print("[FAIL] 只扫到 %d 个 .ahk（预期 >= %d）—— 扫描器退化了，先修工具再信数字"
              % (cur["files_scanned"], MIN_EXPECTED_FILES))
        return 2

    print("AHK 风格/静默失败扫描：")
    print("  文本形态 scope       %s" % ", ".join(TEXT_DIR_SCOPES + TEXT_FILE_SCOPES))
    print("  静默失败/日志 scope  %s  (仅引擎核心)" % ENGINE_ROOT)
    print("  文件数               %3d" % cur["files_scanned"])
    print("  try 无 catch/finally %3d" % cur["try_no_catch"])
    print("  catch 无留痕         %3d" % cur["catch_no_trace"])
    print("  （文档化豁免         %3d  %s）" % (len(f["try_exempt"]), sorted(f["_exempt_hits"])))
    print("  日志 sink            %3d  %s" % (cur["log_sinks"], sorted(f["log_sinks"])))
    print("  BOM 文件             %3d" % cur["bom_files"])
    print("  CRLF 文件            %3d" % cur["crlf_files"])
    print("  tab 缩进文件         %3d  %s" % (cur["tab_indent_files"], f["tab_indent_files"]))
    print("  拼写分裂命中         %3d  %s" % (cur["spelling_drift_hits"],
                                         {k: len(v) for k, v in f["spelling_drift"].items()}))
    if args.verbose:
        for key in ("bom_files", "crlf_files", "try_exempt", "try_no_catch", "catch_no_trace"):
            if f[key]:
                print("  -- %s --" % key)
                for item in f[key]:
                    print("     %s" % item)

    bpath = os.path.join(repo, BASELINE)
    if args.write_baseline:
        with io.open(bpath, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(json.dumps(cur, indent=2, ensure_ascii=False, sort_keys=True) + "\n")
        print("[ok] 基线已写入 %s" % BASELINE)
        return 0

    if args.report:
        return 0

    if not os.path.isfile(bpath):
        print("[FAIL] 基线不存在：%s —— 首次请跑 --write-baseline" % BASELINE)
        return 2
    base = json.loads(io.open(bpath, encoding="utf-8").read())

    worse = []
    print("  --- 与基线对比 ---")
    for key in COUNTER_KEYS:
        b, c = int(base.get(key, 0)), int(cur[key])
        print("  %s %-20s 基线 %3d -> 现在 %3d" % ("  " if c <= b else ">>", key, b, c))
        if c > b:
            worse.append("%s: %d -> %d" % (key, b, c))

    if worse:
        print("[FAIL] 有 %d 类计数**高于基线**（护栏生效）：" % len(worse))
        for w in worse:
            print("    %s" % w)
        print("  若是有意为之，请重新评估并显式更新基线（--write-baseline）。")
        return 1

    print("[ok] 所有计数均不高于基线")
    return 0


if __name__ == "__main__":
    sys.exit(main())
