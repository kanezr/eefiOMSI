#!/usr/bin/env python3
"""The Swift launcher's string catalog (apple/Resources/Localizable.xcstrings) from the game
launcher's translations (crates/omsi-app/locales/app.yml): the English text is the key in
both. Only the texts the Swift sources use are taken. Run after changing either:

    python3 apple/Tools/gen-strings.py
"""
import json
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
YML = os.path.join(ROOT, "crates", "omsi-app", "locales", "app.yml")
OUT = os.path.join(ROOT, "apple", "Resources", "Localizable.xcstrings")

# app.yml's language codes → Apple's
APPLE = {"pt": "pt-BR", "pt-pt": "pt-PT", "zh": "zh-Hans", "zh-tw": "zh-Hant", "tl": "fil"}


def unquote(s):
    s = s.strip()
    if s.startswith('"') and s.endswith('"'):
        return json.loads(s)
    return s


def read_yml(path):
    table, key = {}, None
    for line in open(path, encoding="utf-8"):
        if line.startswith('"') and line.rstrip().endswith(":"):
            key = unquote(line.rstrip()[:-1])
            table[key] = {}
        elif key is not None and line.startswith("  ") and ":" in line:
            lang, _, value = line.strip().partition(":")
            if value.strip():
                table[key][lang.strip()] = unquote(value)
    return table


def swift_texts():
    texts = set()
    lit = re.compile(r'"((?:[^"\\\n]|\\.)*)"')
    for d, _, files in os.walk(os.path.join(ROOT, "apple")):
        for f in files:
            if f.endswith(".swift"):
                for m in lit.finditer(open(os.path.join(d, f), encoding="utf-8").read()):
                    t = m.group(1)
                    if "\\(" not in t and t.strip():
                        texts.add(t.encode().decode("unicode_escape") if "\\" in t else t)
    return texts


def write(strings):
    rows = [json.dumps(k, ensure_ascii=False) + ": " + json.dumps(v, ensure_ascii=False, separators=(",", ":")) for k, v in strings.items()]
    with open(OUT, "w", encoding="utf-8") as f:
        f.write('{\n  "sourceLanguage": "en",\n  "strings": {\n    ' + ",\n    ".join(rows) + '\n  },\n  "version": "1.0"\n}\n')


def main():
    table = read_yml(YML)
    used = swift_texts()
    strings = {}
    for text in sorted(used):
        tr = table.get(text)
        if not tr:
            continue
        strings[text] = {"localizations": {APPLE.get(l, l): {"stringUnit": {"state": "translated", "value": v}} for l, v in sorted(tr.items())}}
    write(strings)
    langs = sorted({l for s in strings.values() for l in s["localizations"]})
    print(f"{len(strings)} of {len(used)} texts translated, {len(langs)} languages: {' '.join(langs)}")


main()
