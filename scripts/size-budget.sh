#!/usr/bin/env bash
# Size ratchet for the Rust sources: no file and no function may grow much past what it
# was when it was recorded in scripts/size-budget.txt, and new ones stay small.
#
#   scripts/size-budget.sh            check the tracked .rs files against the baseline
#   scripts/size-budget.sh --update   lower the baseline to today's sizes (never raises one)
#   scripts/size-budget.sh --list     print every file and its longest function, largest first
#
# Rules (lines counted as in the file, comments and blank lines included):
# * a file may have at most max(1500, its recorded size + 5 %) lines; a new file 1500;
# * a function longer than 300 lines must be recorded (by crate and name, so moving it to
#   another file of the crate keeps its allowance) and may grow at most 5 % past that;
#   a new function, or one that was under 300, may not pass 300.
# A file split into smaller ones, or a long function cut up, is recorded smaller with
# --update; the baseline only ever goes down. Raising it is a decision for a reviewer: edit
# the line in size-budget.txt by hand in the same pull request and say why.
#
# Functions are found by a small lexer (strings, chars and comments blanked, then the braces
# of each `fn` body matched), so it needs nothing but python3 and git.
set -euo pipefail

repo="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
exec python3 -I - "$repo" "$@" <<'PY'
import os, re, subprocess, sys

FILE_CAP, FN_CAP, GROWTH = 1500, 300, 1.05
repo = sys.argv[1]
mode = sys.argv[2] if len(sys.argv) > 2 else "--check"
baseline_path = os.path.join(repo, "scripts", "size-budget.txt")


def blank_code(text):
    """The text with comments, string and char literals replaced by spaces (newlines kept)."""
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        nxt = text[i + 1] if i + 1 < n else ""
        if c == "/" and nxt == "/":
            j = text.find("\n", i)
            j = n if j < 0 else j
            out.append(" " * (j - i))
            i = j
        elif c == "/" and nxt == "*":
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            out.append(re.sub(r"[^\n]", " ", text[i:j]))
            i = j
        elif c == "r" and re.match(r'r#*"', text[i:i + 300]) and not (i and (text[i - 1].isalnum() or text[i - 1] == "_")) or \
                c == "b" and re.match(r'br#*"', text[i:i + 300]) and not (i and (text[i - 1].isalnum() or text[i - 1] == "_")):
            m = re.match(r'b?r(#*)"', text[i:])
            end = text.find('"' + m.group(1), i + m.end())
            j = n if end < 0 else end + 1 + len(m.group(1))
            out.append(re.sub(r"[^\n]", " ", text[i:j]))
            i = j
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            j += 1
            out.append(re.sub(r"[^\n]", " ", text[i:j]))
            i = j
        elif c == "'":
            # a char literal ('x', '\n', '\u{1F600}') or a lifetime ('a)
            m = re.match(r"'(\\(u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.)|[^\\'])'", text[i:i + 12], re.S)
            if m:
                out.append(" " * m.end())
                i += m.end()
            else:
                out.append(c)
                i += 1
        else:
            out.append(c)
            i += 1
    return "".join(out)


FN_RE = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")


def functions(text):
    """(name, first line, line count) of every fn with a body, nested ones included."""
    code = blank_code(text)
    line_starts = [0] + [m.end() for m in re.finditer("\n", code)]

    def line_of(pos):
        lo, hi = 0, len(line_starts) - 1
        while lo < hi:
            mid = (lo + hi + 1) // 2
            if line_starts[mid] <= pos:
                lo = mid
            else:
                hi = mid - 1
        return lo + 1

    found = []
    for m in FN_RE.finditer(code):
        # the body is the first `{` outside parentheses/brackets; a `;` first means none
        j, paren = m.end(), 0
        while j < len(code):
            ch = code[j]
            if ch in "([":
                paren += 1
            elif ch in ")]":
                paren -= 1
            elif ch == ";" and paren == 0:
                j = -1
                break
            elif ch == "{" and paren == 0:
                break
            j += 1
        if j < 0 or j >= len(code):
            continue
        depth, k = 0, j
        while k < len(code):
            if code[k] == "{":
                depth += 1
            elif code[k] == "}":
                depth -= 1
                if depth == 0:
                    break
            k += 1
        start = line_of(m.start())
        found.append((m.group(1), start, line_of(k) - start + 1))
    return found


def crate_of(path):
    parts = path.split("/")
    if "src" in parts:
        return "/".join(parts[: parts.index("src")])
    return os.path.dirname(path)


def measure():
    files = subprocess.run(["git", "-C", repo, "ls-files", "--", "*.rs", ":(exclude)launcher/patches/**"], capture_output=True, text=True, check=True).stdout.split()
    sizes, fns = {}, {}
    for f in files:
        try:
            text = open(os.path.join(repo, f), encoding="utf-8", errors="replace").read()
        except FileNotFoundError:
            continue
        longest = ("-", 0)
        for name, start, length in functions(text):
            if length > longest[1]:
                longest = (name, length)
            if length > FN_CAP:
                key = f"{crate_of(f)}::{name}"
                if length > fns.get(key, (0, ""))[0]:
                    fns[key] = (length, f)
        sizes[f] = (text.count("\n") + (0 if text.endswith("\n") or not text else 1), longest)
    return sizes, fns


def read_baseline():
    files, fns = {}, {}
    if not os.path.exists(baseline_path):
        return files, fns
    for line in open(baseline_path, encoding="utf-8"):
        line = line.rstrip("\n")
        if not line or line.startswith("#"):
            continue
        kind, rest = line.split(" ", 1)
        if kind == "file":
            n, path = rest.split(" ", 1)
            files[path] = int(n)
        elif kind == "fn":
            n, key = rest.split(" ", 2)[:2]
            fns[key] = int(n)
    return files, fns


def write_baseline(files, fns, where):
    with open(baseline_path, "w", encoding="utf-8") as out:
        out.write("# Size ratchet of the Rust sources, checked by scripts/size-budget.sh (see there).\n")
        out.write("# file <lines> <path>: files over 1500 lines may grow 5 % past this.\n")
        out.write("# fn <lines> <crate>::<name> <where>: functions over 300 lines may grow 5 % past this.\n")
        out.write("# Lower it with `scripts/size-budget.sh --update`; raise it only by hand, with a reason.\n")
        for path in sorted(files):
            if files[path] > FILE_CAP:
                out.write(f"file {files[path]} {path}\n")
        for key in sorted(fns):
            out.write(f"fn {fns[key]} {key} {where.get(key, '')}\n".rstrip() + "\n")


sizes, fns = measure()
if mode == "--list":
    for f, (n, (fn_name, fn_len)) in sorted(sizes.items(), key=lambda x: -x[1][0]):
        print(f"{n:6} {fn_len:5} {f} (longest fn: {fn_name})")
    sys.exit(0)

base_files, base_fns = read_baseline()
if mode == "--update":
    new_files = {f: min(n, base_files.get(f, n)) for f, (n, _) in sizes.items()}
    new_fns = {k: min(n, base_fns.get(k, n)) for k, (n, _) in fns.items()}
    if base_files or base_fns:
        # never raise: what is over the allowance stays recorded as it was (and fails the check)
        for f, n in list(new_files.items()):
            if f not in base_files and n > FILE_CAP:
                new_files.pop(f)
        for k, n in list(new_fns.items()):
            if k not in base_fns:
                new_fns.pop(k)
    write_baseline(new_files, new_fns, {k: w for k, (_, w) in fns.items()})
    print(f"wrote {os.path.relpath(baseline_path, repo)}")
    sys.exit(0)
if mode != "--check":
    sys.exit(f"unknown option {mode}")

errors = []
for f, (n, _) in sorted(sizes.items()):
    limit = max(FILE_CAP, int(base_files.get(f, 0) * GROWTH))
    if n > limit:
        what = f"recorded {base_files[f]}" if f in base_files else "a new file"
        errors.append(f"{f}: {n} lines, over its limit of {limit} ({what}). Split it rather than grow it.")
for key, (n, where) in sorted(fns.items()):
    limit = int(base_fns[key] * GROWTH) if key in base_fns else FN_CAP
    if n > limit:
        what = f"recorded {base_fns[key]}" if key in base_fns else "new, or under 300 before"
        errors.append(f"{where}: fn {key.split('::')[-1]} is {n} lines, over its limit of {limit} ({what}). Extract parts of it.")

lowered = sum(1 for f, n in base_files.items() if f in sizes and sizes[f][0] < n) + \
    sum(1 for k, n in base_fns.items() if k in fns and fns[k][0] < n) + \
    sum(1 for k in base_fns if k not in fns)
for e in errors:
    if os.environ.get("GITHUB_ACTIONS"):
        print(f"::error::{e}")
    else:
        print(e)
if errors:
    print(f"\nsize budget: {len(errors)} over (scripts/size-budget.sh explains the rules)")
    sys.exit(1)
print(f"size budget: ok ({len(sizes)} files)" + (f"; {lowered} entries could be lowered with --update" if lowered else ""))
PY
