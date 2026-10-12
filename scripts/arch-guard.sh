#!/usr/bin/env bash
# Architecture guard: which workspace crate may depend on which, and how big the app crate
# may get. The rules live in scripts/arch-rules.txt (see there); this script only checks them.
#
#   scripts/arch-guard.sh            check every crates/*/Cargo.toml and the size of omsi-app
#   scripts/arch-guard.sh --update   lower the recorded size of omsi-app to today's (never raises it)
#   scripts/arch-guard.sh --graph    print the workspace dependency graph, one crate per line
#
# Normal and build dependencies count, target-specific ones included; dev-dependencies do not
# (a test may pull in whatever helps it). A rule `forbid <from> <to>` (globs allowed) fails
# the check when a crate matching <from> depends on one matching <to>; `allow <from> <to>`
# records an existing edge that breaks a rule and is still to be removed. An allowance that is
# no longer needed fails the check as well, so that the line is deleted and the edge cannot
# come back. `app-lines <n>` caps the Rust lines of crates/omsi-app at n + 2 %: the app crate is
# the glue between the others and should only shrink; new subsystems get crates of their own.
#
# Needs nothing but python3 (3.8+) and the checkout.
set -euo pipefail

repo="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
exec python3 -I - "$repo" "$@" <<'PY'
import fnmatch, os, re, sys

repo = sys.argv[1]
mode = sys.argv[2] if len(sys.argv) > 2 else "--check"
rules_path = os.path.join(repo, "scripts", "arch-rules.txt")
APP, GROWTH = "omsi-app", 1.02


def deps_of(manifest):
    """Workspace crates (omsi-*) named in the normal/build dependency tables of one Cargo.toml."""
    found, section = set(), ""
    for raw in open(manifest, encoding="utf-8"):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        m = re.match(r"^\[(.+)\]$", line)
        if m:
            section = m.group(1).strip()
            # [dependencies.omsi-x] / [target.'cfg(..)'.dependencies.omsi-x]
            t = re.match(r"^(?:target\..+\.)?(?:build-)?dependencies\.(omsi-[a-z0-9-]+)$", section)
            if t:
                found.add(t.group(1))
            continue
        if not re.match(r"^(?:target\..+\.)?(?:build-)?dependencies$", section):
            continue
        d = re.match(r"^(omsi-[a-z0-9-]+)\s*=", line)
        if d:
            found.add(d.group(1))
        # a renamed dependency: foo = { package = "omsi-x", ... }
        p = re.search(r'package\s*=\s*"(omsi-[a-z0-9-]+)"', line)
        if p:
            found.add(p.group(1))
    return found


def graph():
    g = {}
    crates_dir = os.path.join(repo, "crates")
    for name in sorted(os.listdir(crates_dir)):
        manifest = os.path.join(crates_dir, name, "Cargo.toml")
        if os.path.exists(manifest):
            g[name] = deps_of(manifest)
    return g


def app_lines():
    total = 0
    for root, _, files in os.walk(os.path.join(repo, "crates", APP)):
        for f in files:
            if f.endswith(".rs"):
                with open(os.path.join(root, f), encoding="utf-8", errors="replace") as fh:
                    total += sum(1 for _ in fh)
    return total


def read_rules():
    forbid, allow, cap, lines = [], [], None, []
    for raw in open(rules_path, encoding="utf-8"):
        lines.append(raw)
        line = raw.split("#", 1)[0].split()
        if not line:
            continue
        if line[0] in ("forbid", "allow") and len(line) == 3:
            (forbid if line[0] == "forbid" else allow).append((line[1], line[2]))
        elif line[0] == "app-lines" and len(line) == 2:
            cap = int(line[1])
        else:
            sys.exit(f"{rules_path}: cannot read line: {raw.rstrip()}")
    return forbid, allow, cap, lines


g = graph()
if mode == "--graph":
    for c, ds in g.items():
        print(f"{c}: {' '.join(sorted(ds)) or '-'}")
    sys.exit(0)

forbid, allow, cap, rule_lines = read_rules()
now = app_lines()
if mode == "--update":
    if cap is not None and now < cap:
        with open(rules_path, "w", encoding="utf-8") as out:
            for raw in rule_lines:
                out.write(re.sub(r"^app-lines\s+\d+", f"app-lines {now}", raw))
        print(f"app-lines lowered from {cap} to {now}")
    else:
        print("nothing to lower")
    sys.exit(0)
if mode != "--check":
    sys.exit(f"unknown option {mode}")

errors, used_allow = [], set()
for c, ds in g.items():
    for d in sorted(ds):
        if d not in g:
            errors.append(f"crates/{c}/Cargo.toml: depends on {d}, which is not a crate of the workspace")
            continue
        broken = [(f, t) for f, t in forbid if fnmatch.fnmatch(c, f) and fnmatch.fnmatch(d, t)]
        if not broken:
            continue
        allowed = [(f, t) for f, t in allow if fnmatch.fnmatch(c, f) and fnmatch.fnmatch(d, t)]
        if allowed:
            used_allow.update(allowed)
            continue
        f, t = broken[0]
        errors.append(f"crates/{c}/Cargo.toml: {c} depends on {d}, against `forbid {f} {t}` in scripts/arch-rules.txt")
for f, t in allow:
    if (f, t) not in used_allow:
        errors.append(f"scripts/arch-rules.txt: `allow {f} {t}` is no longer needed: delete the line so the edge cannot come back")
if cap is not None and now > int(cap * GROWTH):
    errors.append(f"crates/{APP}: {now} lines of Rust, over its cap of {int(cap * GROWTH)} (app-lines {cap} + 2 %). "
                  "Put new subsystems into a crate of their own; omsi-app is the glue.")

for e in errors:
    print(f"::error::{e}" if os.environ.get("GITHUB_ACTIONS") else e)
if errors:
    print(f"\narch guard: {len(errors)} problem(s) (scripts/arch-guard.sh explains the rules)")
    sys.exit(1)
lower = f"; app-lines could be lowered to {now} with --update" if cap is not None and now < cap else ""
print(f"arch guard: ok ({len(g)} crates, {APP} {now} lines){lower}")
PY
