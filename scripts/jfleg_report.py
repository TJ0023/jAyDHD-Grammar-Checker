"""Compare the grammar checker with JFLEG (learner English, 4 human corrections per sentence).

Usage:
  python3 scripts/jfleg_report.py prepare  <jfleg dir> <dev|test> <work dir>
  (run `flag_dump <work dir>/<split>.txt > <work dir>/<split>_flags.tsv` in between)
  python3 scripts/jfleg_report.py score    <jfleg dir> <dev|test> <work dir>

What it counts, per flag:
  * "agrees"      the flagged words were changed by at least one of the four human corrections
  * "clean line"  the flag is on a sentence where all four humans changed nothing (a certain false alarm)
  * "unconfirmed" everything else: either a false alarm, or a real error the humans fixed another way
JFLEG is learner English, so most of its errors are ones this checker does not aim at. The numbers
that matter are the false alarms and the agreement rate of the flags it does give.
"""
import re
import sys
import difflib
from collections import Counter, defaultdict


def detok(s: str) -> str:
    s = s.strip()
    s = re.sub(r"\bca n't\b", "can't", s)
    s = re.sub(r"\bwo n't\b", "won't", s)
    s = re.sub(r"\b(\w+) n't\b", r"\1n't", s)
    s = re.sub(r" (n't|'s|'re|'ve|'ll|'d|'m)\b", r"\1", s)
    s = re.sub(r" ([,.;:!?%)\]])", r"\1", s)
    s = re.sub(r"([(\[$]) ", r"\1", s)
    s = s.replace("`` ", '"').replace(" ''", '"')
    return re.sub(r"\s+", " ", s).strip()


def words(s: str):
    return [(m.group(), m.start(), m.end()) for m in re.finditer(r"\w+|[^\w\s]", s)]


def changed_spans(src: str, ref: str):
    """Character spans in `src` that `ref` changes (an insertion counts as the neighbouring words)."""
    a, b = words(src), words(ref)
    sm = difflib.SequenceMatcher(a=[w[0].lower() for w in a], b=[w[0].lower() for w in b], autojunk=False)
    spans = []
    for tag, i1, i2, _, _ in sm.get_opcodes():
        if tag == "equal":
            continue
        if i1 == i2:  # insertion: mark the word before and after
            lo, hi = max(i1 - 1, 0), min(i1, len(a) - 1)
        else:
            lo, hi = i1, i2 - 1
        if a:
            spans.append((a[lo][1], a[hi][2]))
    return spans


def read(path):
    with open(path, encoding="utf-8") as f:
        return [l.rstrip("\n") for l in f]


def main():
    mode, jf, split, work = sys.argv[1:5]
    src = [detok(l) for l in read(f"{jf}/{split}/{split}.src")]
    if mode == "prepare":
        with open(f"{work}/{split}.txt", "w", encoding="utf-8") as f:
            f.write("\n".join(src) + "\n")
        print(len(src), "sentences written")
        return
    refs = [[detok(l) for l in read(f"{jf}/{split}/{split}.ref{k}")] for k in range(4)]
    flags = defaultdict(list)
    for line in read(f"{work}/{split}_flags.tsv"):
        c = line.split("\t")
        n, b0, b1 = int(c[0]), int(c[1]), int(c[2])
        text = src[n]
        # byte offsets from the checker -> character offsets
        raw = text.encode("utf-8")
        s0, s1 = len(raw[:b0].decode("utf-8", "ignore")), len(raw[:b1].decode("utf-8", "ignore"))
        flags[n].append((s0, s1, c[3], c[5], c[6] if len(c) > 6 else ""))
    clean = {i for i in range(len(src)) if all(refs[k][i] == src[i] for k in range(4))}
    tot = Counter()
    per_rule = defaultdict(Counter)
    unconfirmed = []
    for n, fl in flags.items():
        spans = [sp for k in range(4) for sp in changed_spans(src[n], refs[k][n])]
        for s0, s1, rule, flagged, fix in fl:
            hit = any(s0 < e and b < s1 + 1 and s0 <= e and b <= s1 for b, e in spans)
            kind = "agrees" if hit else ("clean line" if n in clean else "unconfirmed")
            tot[kind] += 1
            per_rule[rule][kind] += 1
            if kind != "agrees":
                unconfirmed.append((kind, rule, n, flagged, fix, src[n]))
    total = sum(tot.values())
    print(f"{split}: {len(src)} sentences ({len(clean)} where all four humans changed nothing)")
    print(f"flags: {total} on {len(flags)} sentences; per 100 sentences: {100 * total / len(src):.1f}")
    for k in ("agrees", "unconfirmed", "clean line"):
        print(f"  {k:12} {tot[k]:4}  ({100 * tot[k] / max(total, 1):.0f}%)")
    print("\nby rule (agrees / unconfirmed / clean line):")
    for rule, c in sorted(per_rule.items(), key=lambda kv: -sum(kv[1].values())):
        print(f"  {rule:26} {c['agrees']:3} / {c['unconfirmed']:3} / {c['clean line']:3}")
    with open(f"{work}/{split}_review.tsv", "w", encoding="utf-8") as f:
        for kind, rule, n, flagged, fix, sentence in sorted(unconfirmed, key=lambda x: (x[1], x[2])):
            f.write(f"{kind}\t{rule}\t{n}\t{flagged}\t{fix}\t{sentence}\n")
    print(f"\n{len(unconfirmed)} flags to review are in {work}/{split}_review.tsv")


main()
