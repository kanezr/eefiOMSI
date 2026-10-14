"""Checks a pull request's changes to the interface translations, for
.github/workflows/pr_checks.yml.

    check_locales.py <app.yml before> <app.yml after>

The English text is the key of every entry (see `omsi_ui::tr`), so a translation PR should
only add or correct texts. Errors (the check fails):

* the file does not parse, or a key is written twice (YAML keeps the last one silently);
* a translation added or changed here has other placeholders than its key (`%{packs}`,
  `{}`): the text the game fills in would be lost or garbled;
* a key is removed whose text the code still uses: that text loses every language.

Warnings (shown on the pull request, the check passes):

* an existing translation is changed - often a correction, sometimes another contributor's
  text overwritten by mistake;
* a new key is not a text the code uses (a typo, a missing `...`, a trailing space), so it
  is never shown.

Annotations go to stdout in the GitHub Actions format, a summary to $GITHUB_STEP_SUMMARY.
"""
import os
import re
import sys
from pathlib import Path

import yaml

FILE = "crates/omsi-app/locales/app.yml"
PLACEHOLDER = re.compile(r"%\{\w+\}|(?<!%)\{[^{}]*\}")


class DuplicateKey(Exception):
    pass


class Loader(yaml.SafeLoader):
    pass


def mapping(loader, node, deep=False):
    out = {}
    for k, v in node.value:
        key = loader.construct_object(k, deep=deep)
        if key in out:
            raise DuplicateKey(f"{key!r} is written twice (line {k.start_mark.line + 1})")
        out[key] = loader.construct_object(v, deep=deep)
    return out


Loader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, mapping)


def load(path: str) -> dict:
    with open(path, encoding="utf-8") as f:
        data = yaml.load(f, Loader=Loader) or {}
    return {k: v for k, v in data.items() if k != "_version"}


def code_strings(root: Path) -> set[str]:
    """Every string literal of the Rust code, unescaped as far as texts need it."""
    out = set()
    for p in root.glob("crates/**/*.rs"):
        for lit in re.findall(r'"((?:[^"\\]|\\.)*)"', p.read_text(encoding="utf-8", errors="replace")):
            out.add(lit.replace('\\"', '"').replace("\\n", "\n").replace("\\\\", "\\"))
    return out


def line_of(lines: list[str], key: str, lang: str | None = None) -> int:
    """The line of `key` in the new file (and of its `lang:` entry), for the annotation."""
    quoted = [f'"{key}":', f"'{key}':", f"{key}:"]
    for i, line in enumerate(lines):
        if any(line.startswith(q) for q in quoted):
            if lang is None:
                return i + 1
            for j in range(i + 1, min(i + 60, len(lines))):
                if not lines[j].startswith(" "):
                    break
                if lines[j].strip().startswith(f"{lang}:"):
                    return j + 1
            return i + 1
    return 1


def main() -> int:
    before_path, after_path = sys.argv[1], sys.argv[2]
    errors, warnings = [], []
    try:
        before = load(before_path)
    except Exception:
        before = {}  # the base had none, or a broken one: judge only the new file
    try:
        after = load(after_path)
    except (DuplicateKey, yaml.YAMLError) as e:
        print(f"::error file={FILE}::{FILE} does not load: {e}")
        return 1
    lines = Path(after_path).read_text(encoding="utf-8").splitlines()
    code = code_strings(Path("."))

    changed_langs: dict[str, int] = {}
    for key, langs in after.items():
        if not isinstance(langs, dict):
            errors.append((line_of(lines, key), f"{key!r} is not a list of languages"))
            continue
        old = before.get(key) or {}
        for lang, text in langs.items():
            if old.get(lang) == text:
                continue
            if sorted(PLACEHOLDER.findall(str(key))) != sorted(PLACEHOLDER.findall(str(text))):
                errors.append((line_of(lines, key, lang), f"{lang}: the placeholders of {key!r} ({' '.join(PLACEHOLDER.findall(str(key))) or 'none'}) are not in the translation {text!r}"))
            if lang in old:
                changed_langs[lang] = changed_langs.get(lang, 0) + 1
                warnings.append((line_of(lines, key, lang), f"{lang}: an existing translation of {key!r} is changed from {old[lang]!r} to {text!r}"))
        if key not in before and key not in code:
            warnings.append((line_of(lines, key), f"{key!r} is not a text the code uses, so this entry is never shown"))
    for key in before:
        if key not in after and key in code:
            errors.append((1, f"{key!r} is removed, but the code still uses it: it loses every language"))

    for line, msg in errors:
        print(f"::error file={FILE},line={line}::{msg}")
    for line, msg in warnings[:50]:
        print(f"::warning file={FILE},line={line}::{msg}")
    added = sum(len(v) for k, v in after.items() if isinstance(v, dict)) - sum(len(v) for v in before.values() if isinstance(v, dict))
    summary = [
        "### Translations",
        f"- entries: {added:+} texts, {len(after) - len(before):+} keys",
        f"- existing translations changed: {sum(changed_langs.values())}" + (f" ({', '.join(f'{l} {n}' for l, n in sorted(changed_langs.items()))})" if changed_langs else ""),
        f"- errors: {len(errors)}, warnings: {len(warnings)}" + (" (the first 50 are annotated)" if len(warnings) > 50 else ""),
    ]
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as f:
            f.write("\n".join(summary) + "\n")
    print("\n".join(summary), file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
