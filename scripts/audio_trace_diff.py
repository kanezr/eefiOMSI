"""Compare two audio traces (OMSI_AUDIO_TRACE) as what is heard, not as the order it was asked
in: the first step whose requests differ, the sound files started more or less often, and
how far apart the two mixes are.

    python3 -I scripts/audio_trace_diff.py a.trace b.trace [a.wav b.wav]
    python3 -I scripts/audio_trace_diff.py --window a.trace b.trace

Exits 0 when the two traces ask for the same sounds, 1 otherwise. Standard library only;
used by scripts/audio-trace.sh compare.

Some of the game's sounds are started from hash maps (the ambience's layers, a vehicle's
sound set), whose order differs from run to run of one binary. So voices are named by what
they play and when they started (`<file>@<step>#<n>`) instead of their number, the lines of
a step are sorted, and the place in the clip a loop starts from (drawn from one sequence by
every loop in the order they start) is left out. What remains is the same from run to run.
The mixes differ by the order the voices are summed in (a bit or two) and by those start
places; their difference is printed, not judged.
"""

import collections
import re
import sys
import wave

ID = re.compile(r"^(\+|~|-|x|fx) (\d+)( |$)")


def canonical(path):
    """The trace as steps of sorted lines, with voices named by file and start step."""
    with open(path, encoding="utf-8", errors="replace") as f:
        lines = [l.rstrip("\n") for l in f]
    raw, cur = [], []
    for line in lines:
        if line.startswith("L "):
            raw.append(cur)
            cur = []
        cur.append(line)
    raw.append(cur)
    steps, names = [], {}
    for n, step in enumerate(raw):
        # the voices started in this step, named in the order of what they play
        started = []
        for line in step:
            m = ID.match(line)
            if m and m.group(1) == "+":
                file, _, params = line[m.end() :].partition(" g=")
                params = re.sub(r" from=\d+$", "", "g=" + params)
                started.append((file, params, m.group(2)))
        seen = collections.Counter()
        for file, params, vid in sorted(started):
            seen[file] += 1
            names[vid] = f"{file}@{n}#{seen[file]}"
        out = []
        for line in step:
            m = ID.match(line)
            if line.startswith("L "):
                out.append(re.sub(r"^L \d+ ", "L ", line))
            elif not m:
                out.append(line)
            elif m.group(1) == "+":
                params = re.sub(r" from=\d+$", "", "g=" + line[m.end() :].partition(" g=")[2])
                out.append(f"+ {names[m.group(2)]} {params}")
            else:
                vid = m.group(2)
                out.append(f"{m.group(1)} {names.get(vid, 'v' + vid)} {line[m.end():]}".rstrip())
        steps.append(sorted(out))
    return steps


def starts(steps):
    files = collections.Counter()
    for step in steps:
        for line in step:
            if line.startswith("+ "):
                files[line[2:].partition(" g=")[0].rsplit("@", 1)[0]] += 1
    return files


def heard(path):
    """What a window's trace played, whatever the frame: starts per file (per slot for the
    ambience's takes, which it picks by the wall clock), and the streams (the radio)."""
    out = collections.Counter()
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            m = ID.match(line)
            if m and m.group(1) == "+":
                file = line[m.end() :].partition(" g=")[0]
                if "/ambience/" in file:
                    file = file.rsplit("/", 1)[0] + "/*"
                out[file] += 1
            elif m and m.group(1) == "fx":
                out["(radio effects set)"] += 1
    return out


def window(a_path, b_path):
    """Two window traces: the sounds only one of them played, and the counts."""
    a, b = heard(a_path), heard(b_path)
    only = sorted(f for f in set(a) | set(b) if (a[f] == 0) != (b[f] == 0))
    print(f"voices started: {sum(a.values())} / {sum(b.values())}, sounds: {len(a)} / {len(b)}, played by one only: {len(only)}")
    for f in only:
        print(f"  {a[f]:4d} / {b[f]:4d}  {f}")
    print("played by both:")
    for f in sorted(set(a) | set(b), key=lambda f: (-max(a[f], b[f]), f)):
        if f not in only:
            print(f"  {a[f]:4d} / {b[f]:4d}  {f}")
    sys.exit(0 if not only else 1)


def mix(path):
    with wave.open(path, "rb") as w:
        data = w.readframes(w.getnframes())
    return [int.from_bytes(data[i : i + 2], "little", signed=True) for i in range(0, len(data) - 1, 2)]


def main():
    if sys.argv[1] == "--window":
        window(sys.argv[2], sys.argv[3])
    a, b = canonical(sys.argv[1]), canonical(sys.argv[2])
    same = a == b
    print(f"steps: {len(a) - 1} / {len(b) - 1}, requests {'the same' if same else 'DIFFER'}")
    for n, (sa, sb) in enumerate(zip(a, b)):
        if sa != sb:
            print(f"first difference in step {n} ({sa[0] if sa else '-'}):")
            ca, cb = collections.Counter(sa), collections.Counter(sb)
            for line in sorted((ca - cb).elements())[:12]:
                print(f"  a: {line}")
            for line in sorted((cb - ca).elements())[:12]:
                print(f"  b: {line}")
            break
    else:
        if len(a) != len(b):
            print("one trace is a longer copy of the other")
    fa, fb = starts(a), starts(b)
    moved = sorted((f for f in set(fa) | set(fb) if fa[f] != fb[f]), key=lambda f: (-abs(fa[f] - fb[f]), f))
    print(f"voices started: {sum(fa.values())} / {sum(fb.values())}, files started a different number of times: {len(moved)}")
    for f in moved[:15]:
        print(f"  {fa[f]:4d} / {fb[f]:4d}  {f}")
    if len(sys.argv) > 4:
        try:
            ma, mb = mix(sys.argv[3]), mix(sys.argv[4])
        except (OSError, EOFError, wave.Error) as e:
            print(f"mix: {e}")
        else:
            n = min(len(ma), len(mb))
            diff = [abs(x - y) for x, y in zip(ma[:n], mb[:n])]
            first = next((i for i, d in enumerate(diff) if d), None)
            if first is None and len(ma) == len(mb):
                print("mix: identical")
            else:
                at = "-" if first is None else f"{first / 2 / 48000:.3f} s"
                print(f"mix: {len(ma)} / {len(mb)} samples, first difference at {at}, largest {max(diff, default=0)}, mean {sum(diff) / max(n, 1):.3f} (of 32767)")
    sys.exit(0 if same else 1)


if __name__ == "__main__":
    main()
