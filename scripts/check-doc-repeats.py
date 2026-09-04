#!/usr/bin/env python3
# Flags a phrase of three to seven words repeated back to back, the one defect that
# survived five rounds of human review of docs/keysets.md. Fenced blocks are skipped:
# repeated tokens are normal in frame dumps and command output.
#
# Deliberately narrow. Line-length and short-line checks were measured against this
# repo and produced 103 false positives with no true ones, so they are not here.
import re, sys

def strip_fences(text):
    out, fenced = [], False
    for line in text.split("\n"):
        if line.lstrip().startswith("```"):
            fenced = not fenced
            continue
        out.append("" if fenced else line)
    return out

bad = 0
for path in sys.argv[1:]:
    lines = strip_fences(open(path).read())
    joined = " ".join(l.strip() for l in lines)
    for m in re.finditer(r"\b((?:\w+ ){2,6}\w+) \1\b", joined):
        line = next((i for i, l in enumerate(lines, 1) if m.group(1) in l), 0)
        print(f"{path}:{line}: phrase repeated back to back: '{m.group(1)}'")
        bad += 1
if bad:
    print(f"\n{bad} repeated phrase(s). Each is almost certainly an edit appending text already present.")
sys.exit(1 if bad else 0)
