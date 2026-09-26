#!/usr/bin/env python3
"""Remove artefact invisible characters from a text file.

Refuses binary and container formats outright: running a text transform over a
PNG or a DOCX and writing the result back corrupts the file.

Characters that are invisible but load-bearing (emoji joiners, variation
selectors, script-required joiners, bidi controls in right-to-left documents)
are preserved. Pass --include-protected only after reviewing the inspection
report and deciding per file.

Exit codes: 0 written or nothing to do, 1 refused or failed.

Examples:
  python3 clean_text.py notes.md
  python3 clean_text.py notes.md -o clean.md --normalise-spaces
  python3 clean_text.py notes.md --stdout
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import text_marks as tm
from common import SafetyError, atomic_write, default_output, require_text, sniff


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("input", help="text file to clean")
    p.add_argument("-o", "--output", help="output path (default: input.cleaned.ext)")
    p.add_argument("--stdout", action="store_true", help="write to stdout instead of a file")
    p.add_argument("--normalise-spaces", action="store_true",
                   help="convert exotic spaces to U+0020; changes visible layout")
    p.add_argument("--include-protected", action="store_true",
                   help="also remove load-bearing invisibles. Can corrupt emoji and RTL text")
    p.add_argument("--force", action="store_true", help="overwrite an existing output path")
    args = p.parse_args(argv)

    src = Path(args.input)
    if not src.is_file():
        print(f"error: {src} is not a file", file=sys.stderr)
        return 1

    try:
        file_type = sniff(src)
        require_text(src, file_type)
    except SafetyError as exc:
        print(f"refused: {exc}", file=sys.stderr)
        return 1

    text = src.read_text(encoding="utf-8", errors="strict")
    cleaned, removed = tm.clean(
        text,
        normalise_spaces=args.normalise_spaces,
        include_protected=args.include_protected,
    )

    protected_kept = [m for m in tm.scan(text) if m.protected and not args.include_protected]

    if not removed:
        print(f"{src}: no removable marks found; no file written")
        if protected_kept:
            print(f"  {len(protected_kept)} load-bearing invisible character(s) preserved")
        return 0

    if args.stdout:
        sys.stdout.write(cleaned)
    else:
        dest = Path(args.output) if args.output else default_output(src)
        try:
            atomic_write(dest, cleaned.encode("utf-8"), force=args.force)
        except SafetyError as exc:
            print(f"refused: {exc}", file=sys.stderr)
            return 1
        print(f"{src} -> {dest}")

    by_kind: dict[str, int] = {}
    for m in removed:
        by_kind[f"{m.codepoint} {m.name}"] = by_kind.get(f"{m.codepoint} {m.name}", 0) + 1
    print("removed:")
    for label, n in sorted(by_kind.items()):
        print(f"  {n} x {label}")
    if protected_kept:
        print(f"preserved: {len(protected_kept)} load-bearing invisible character(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
