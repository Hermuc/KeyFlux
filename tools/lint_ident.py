# -*- coding: utf-8 -*-
"""
AHK v2 identifier-collision lint (zero desktop impact).

WHY THIS EXISTS
---------------
AHK identifiers are CASE-INSENSITIVE. If a function-local variable has the same
name as a function (or a built-in), then any call like `W(...)` inside that
function resolves to the LOCAL VARIABLE, not the function. The failure is a
RUNTIME error ("This local variable has not been assigned a value") that
`/Validate` cannot see, because the syntax is perfectly legal.

This exact bug shipped in probe.ahk rev3: `for i, w in list` inside Launch()
collided with the logger function `W`, so the very first successful T0 detection
aborted with the misleading message above. Proven with a positive+negative
control in shadow.ahk, both variants, same interpreter (AHK 2.0.19).

WHAT IT CHECKS
--------------
For every function defined in the file:
  ERROR  a local variable name == a function name defined in this file
         AND that name is called somewhere inside the same function
  WARN   a local variable name == a function name defined in this file (no call)
  ERROR  a `global` variable name == a function name defined in this file
  WARN   a local variable name == a known AHK built-in (Log, Trim, Format, ...)

Exit code 0 = clean, 1 = findings. Run it BEFORE every probe run.
"""
import re
import sys
import os

KEYWORDS = {
    "if", "else", "for", "while", "loop", "until", "switch", "case", "default",
    "try", "catch", "finally", "return", "break", "continue", "throw", "global",
    "local", "static", "class", "new", "not", "and", "or", "is", "in", "contains",
    "super", "this", "get", "set", "__new", "__init", "__delete", "do", "goto",
    "gosub", "exit", "exitapp", "var", "fileappend", "msgbox",
}

# Built-ins that are plausible variable names AND are commonly called.
# Curated, not exhaustive: the file-defined-function check is the real gate.
BUILTINS = {
    "log", "ln", "exp", "sqrt", "abs", "ceil", "floor", "round", "mod", "min",
    "max", "sin", "cos", "tan", "asin", "acos", "atan", "format", "sort",
    "trim", "ltrim", "rtrim", "substr", "instr", "strlen", "strsplit",
    "integer", "float", "number", "string", "ismatch", "regexmatch", "type",
    "objget", "objset", "objhas", "isobject", "isnumber", "isalnum", "isspace",
    "random", "clamp", "array", "map", "buffer", "chr", "ord",
}

FUNC_DEF = re.compile(r'^\s*([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\)\s*\{\s*$')
ASSIGN = re.compile(r'(?<![.:\w])([A-Za-z_][A-Za-z0-9_]*)\s*(?::=|\+=|-=|\*=|/=|\.=|\|=|&=|\^=)')
FOR_VARS = re.compile(r'\bfor\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:,\s*([A-Za-z_][A-Za-z0-9_]*)\s*)?\bin\b')
CALL = re.compile(r'(?<![.\w])([A-Za-z_][A-Za-z0-9_]*)\s*\(')
GLOBAL_DECL = re.compile(r'^\s*global\s+(.+)$')


def strip_comment(line):
    """Remove `;` comments, honouring simple double-quoted strings."""
    out, in_str, i = [], False, 0
    while i < len(line):
        ch = line[i]
        if ch == '"':
            in_str = not in_str
            out.append(ch)
        elif ch == ';' and not in_str:
            break
        else:
            out.append(ch)
        i += 1
    return "".join(out)


def load(path):
    with open(path, "r", encoding="utf-8-sig") as f:
        raw = f.readlines()
    clean = [strip_comment(l.rstrip("\n")) for l in raw]
    return raw, clean


def find_functions(clean):
    """Return {name: (start_line_idx, end_line_idx_exclusive)} via brace matching."""
    funcs = {}
    i = 0
    while i < len(clean):
        m = FUNC_DEF.match(clean[i])
        if m and m.group(1).lower() not in KEYWORDS:
            name = m.group(1)
            depth = clean[i].count("{") - clean[i].count("}")
            j = i + 1
            while j < len(clean) and depth > 0:
                depth += clean[j].count("{") - clean[j].count("}")
                j += 1
            funcs[name] = (i, j)
            i = j
        else:
            i += 1
    return funcs


def analyse(path):
    raw, clean = load(path)
    funcs = find_functions(clean)
    func_names_lower = {n.lower(): n for n in funcs}
    findings = []

    for name, (s, e) in funcs.items():
        body = clean[s + 1:e]
        params = [p.strip().split(":=")[0].strip().lstrip("*").strip()
                  for p in FUNC_DEF.match(clean[s]).group(2).split(",")]
        params = [p for p in params if p]
        local_names = {}
        global_names = {}

        for p in params:
            local_names.setdefault(p.lower(), p)

        for idx, line in enumerate(body):
            for m in ASSIGN.finditer(line):
                local_names.setdefault(m.group(1).lower(), m.group(1))
            for m in FOR_VARS.finditer(line):
                for g in m.groups():
                    if g:
                        local_names.setdefault(g.lower(), g)
            gm = GLOBAL_DECL.match(line)
            if gm:
                for g in gm.group(1).split(","):
                    g = g.strip().rstrip(";").strip()
                    if re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', g or ""):
                        global_names.setdefault(g.lower(), g)

        calls = set()
        for line in body:
            for m in CALL.finditer(line):
                nm = m.group(1)
                if nm.lower() not in KEYWORDS:
                    calls.add(nm.lower())

        for low, orig in sorted(local_names.items()):
            if low in func_names_lower:
                called = low in calls
                findings.append((
                    "ERROR" if called else "WARN", name, orig,
                    func_names_lower[low],
                    ("local variable shadows the function '%s' AND '%s(' is called here "
                     "-> the call resolves to the variable; runtime failure") % (func_names_lower[low], orig)
                    if called else
                    ("local variable shadows the function '%s' (latent hazard)") % func_names_lower[low],
                    raw, s, e))
            elif low in BUILTINS:
                findings.append((
                    "WARN", name, orig, orig,
                    "local variable shadows the built-in '%s'" % orig,
                    raw, s, e))

        for low, orig in sorted(global_names.items()):
            if low in func_names_lower:
                findings.append((
                    "ERROR", name, orig, func_names_lower[low],
                    "global variable shadows the function '%s'" % func_names_lower[low],
                    raw, s, e))

    return funcs, findings, raw


def line_of(raw, funcs, name, bodytext_idx):
    return None


def main(paths):
    total = 0
    for path in paths:
        funcs, findings, raw = analyse(path)
        print("=" * 78)
        print("FILE %s  functions=%d %s" % (path, len(funcs), sorted(funcs.keys())))
        if not findings:
            print("  CLEAN - no identifier collisions")
        for sev, fn, var, target, msg, rawlines, s, e in findings:
            total += 1
            # locate the offending line for a precise report
            where = ""
            for idx in range(s + 1, e):
                if re.search(r'(?<![.\w])' + re.escape(var) + r'\s*(?::=|(?::=|\+=|\.=))', rawlines[idx]) \
                   or re.search(r'\bfor\b[^\n]*\b' + re.escape(var) + r'\b', rawlines[idx]):
                    where = "  line %d: %s" % (idx + 1, rawlines[idx].strip()[:110])
                    break
            print("  [%s] %s() var='%s' -> %s" % (sev, fn, var, msg))
            if where:
                print("       " + where.strip())
            if sev == "ERROR":
                print("       FIX: rename the variable (e.g. '%s' -> '%sX') "
                      "so it no longer matches the function name" % (var, var.rstrip('0123456789') or var))
    print("=" * 78)
    print("TOTAL_FINDINGS=%d" % total)
    return 1 if any(f[0] == "ERROR" for p in paths for f in analyse(p)[1]) else 0


if __name__ == "__main__":
    args = sys.argv[1:]
    if not args:
        args = ["probe.ahk", "target_open.ahk", "target_folder.ahk",
                "target_msgbox.ahk", "target_save.ahk"]
    sys.exit(main(args))
