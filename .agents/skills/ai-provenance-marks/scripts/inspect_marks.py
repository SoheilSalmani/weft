#!/usr/bin/env python3
"""Inspect a file or directory for AI provenance marks. Never mutates anything.

Exit codes: 0 inspected successfully (with or without findings),
            1 the input could not be inspected safely.

Examples:
  python3 inspect_marks.py notes.md
  python3 inspect_marks.py shot.png --json
  python3 inspect_marks.py ./content --recursive --json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import container_marks as cm
import text_marks as tm
from common import (
    CONFIRMED, INFORMATIONAL, PROBABLE, Finding, Report, SafetyError,
    TEXT_TYPES, c2pa_state, emit, sniff, tool_available,
)

SUPPORTED = {"text", "markdown", "html", "svg", "code", "png", "jpeg", "docx", "odt", "pdf"}


def inspect_path(path: Path) -> Report:
    file_type = sniff(path)
    report = Report(path=str(path), file_type=file_type)

    if file_type not in SUPPORTED:
        report.warnings.append(
            f"'{file_type}' is not a supported format; no inspection was attempted"
        )
        return report

    if file_type in TEXT_TYPES:
        text = path.read_text(encoding="utf-8", errors="replace")
        marks = tm.scan(text)
        for entry in tm.summarise(marks)["marks"]:
            report.add(Finding(
                kind=f"unicode:{entry['category']}",
                detail=f"{entry['codepoint']} {entry['name']}",
                confidence=CONFIRMED,
                count=entry["count"],
                removable=not entry["protected"],
                protected=entry["protected"],
                note=entry["reason"],
            ))
        note = tm.normalisation_note(text)
        if note:
            report.warnings.append(note)
        data = path.read_bytes()
        if file_type == "markdown":
            report.findings.extend(cm.markdown_inspect(data))
        elif file_type == "html":
            report.findings.extend(cm.html_inspect(data))
        elif file_type == "svg":
            report.findings.extend(cm.svg_inspect(data))
        return report

    data = path.read_bytes()
    if file_type == "png":
        report.findings.extend(cm.png_inspect(data))
    elif file_type == "jpeg":
        report.findings.extend(cm.jpeg_inspect(data))
    elif file_type in {"docx", "odt"}:
        report.findings.extend(cm.office_inspect(path, file_type))
    elif file_type == "pdf":
        if tool_available("exiftool"):
            report.warnings.append("PDF inspection uses exiftool")
        else:
            report.unavailable.append(
                "PDF metadata cannot be inspected without exiftool; state is unknown, not absent"
            )

    if file_type in {"png", "jpeg", "pdf"}:
        state, detail = c2pa_state(path)
        if state == "present":
            report.add(Finding("c2pa", detail, CONFIRMED))
        elif state == "unknown":
            report.unavailable.append(detail)

    # Stated on every media file, because it is a property of the standard.
    if file_type in {"png", "jpeg"}:
        report.unavailable.append(
            "pixel-domain watermarks and C2PA soft bindings cannot be detected by this tool"
        )
    return report


SKIP_DIRS = {".git", "node_modules", "dist", "build", ".next", "__pycache__", ".venv", "vendor"}


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("path", help="file or directory to inspect")
    p.add_argument("--json", action="store_true", help="machine-readable output")
    p.add_argument("--recursive", action="store_true", help="descend into a directory")
    args = p.parse_args(argv)

    root = Path(args.path)
    if not root.exists():
        print(f"error: {root} does not exist", file=sys.stderr)
        return 1

    if root.is_dir():
        if not args.recursive:
            print(f"error: {root} is a directory; pass --recursive to inspect it", file=sys.stderr)
            return 1
        results, errors, skipped = [], [], []
        for f in sorted(root.rglob("*")):
            if any(part in SKIP_DIRS for part in f.parts):
                continue
            # Skip symlinks rather than following them. A link inside the tree
            # can point anywhere, and an audit of ./content should not end up
            # reading /etc/passwd. Reported, not silently ignored.
            if f.is_symlink():
                skipped.append({"path": str(f), "reason": "symlink not followed"})
                continue
            if not f.is_file():
                continue
            try:
                results.append(inspect_path(f).to_dict())
            except SafetyError as exc:
                errors.append({"path": str(f), "error": str(exc)})
        payload = {
            "root": str(root),
            "inspected": len(results),
            "with_findings": sum(1 for r in results if r["findings"]),
            "errors": errors,
            "skipped": skipped,
            "files": results,
        }
        if args.json:
            json.dump(payload, sys.stdout, indent=2, ensure_ascii=False)
            sys.stdout.write("\n")
        else:
            print(f"{root}: inspected {payload['inspected']} file(s), "
                  f"{payload['with_findings']} with findings, {len(errors)} error(s), "
                  f"{len(skipped)} skipped")
            for r in results:
                if r["findings"]:
                    print(f"  {r['path']} [{r['file_type']}]: {len(r['findings'])} finding(s)")
            for e in errors:
                print(f"  error {e['path']}: {e['error']}")
            for sk in skipped:
                print(f"  skipped {sk['path']}: {sk['reason']}")
        return 0

    try:
        emit(inspect_path(root).to_dict(), args.json)
    except SafetyError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
