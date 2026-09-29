#!/usr/bin/env python3
"""Remove AI provenance metadata from a container file.

Modes:
  provenance    (default) C2PA manifests, XMP packets and AI generator tags.
                EXIF is preserved, so orientation, colour and timestamps stay.
  all-metadata  broader strip including EXIF. Say so in the report; this is a
                different request from "remove the AI provenance".

Encoded pixel payloads are never re-encoded: PNG IDAT and JPEG scan data are
copied byte for byte.

Exit codes: 0 written or nothing to do, 1 refused or failed.

Examples:
  python3 clean_container.py shot.png
  python3 clean_container.py shot.jpg --mode all-metadata
  python3 clean_container.py deck.docx -o clean.docx
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import container_marks as cm
from common import SafetyError, atomic_write, c2pa_state, default_output, sniff, tool_available

HANDLERS = {"png", "jpeg", "svg", "html", "markdown", "docx", "odt"}


def clean_bytes(path: Path, file_type: str, mode: str) -> cm.CleanResult:
    if file_type in {"docx", "odt"}:
        return cm.office_clean(path, file_type, mode)
    data = path.read_bytes()
    return {
        "png": cm.png_clean,
        "jpeg": cm.jpeg_clean,
        "svg": cm.svg_clean,
        "html": cm.html_clean,
        "markdown": cm.markdown_clean,
    }[file_type](data, mode)


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("input", help="file to clean")
    p.add_argument("-o", "--output", help="output path (default: input.cleaned.ext)")
    p.add_argument("--mode", choices=["provenance", "all-metadata"], default="provenance",
                   help="what to remove (default: provenance)")
    p.add_argument("--force", action="store_true", help="overwrite an existing output path")
    args = p.parse_args(argv)

    src = Path(args.input)
    if not src.is_file():
        print(f"error: {src} is not a file", file=sys.stderr)
        return 1

    try:
        file_type = sniff(src)
    except SafetyError as exc:
        print(f"refused: {exc}", file=sys.stderr)
        return 1

    if file_type == "pdf":
        if not tool_available("exiftool"):
            print("refused: PDF metadata removal requires exiftool, which is not installed.\n"
                  "  Hand-written PDF surgery risks corrupting the document, so nothing was changed.",
                  file=sys.stderr)
            return 1
        print("error: PDF cleaning via exiftool is not implemented in this version", file=sys.stderr)
        return 1

    if file_type not in HANDLERS:
        print(f"refused: '{file_type}' is not a supported container format; nothing was changed",
              file=sys.stderr)
        return 1

    before_c2pa = c2pa_state(src) if file_type in {"png", "jpeg"} else (None, None)

    try:
        result = clean_bytes(src, file_type, args.mode)
    except SafetyError as exc:
        print(f"refused: {exc}", file=sys.stderr)
        return 1

    if not result.changed:
        print(f"{src}: no removable provenance metadata found; no file written")
        return 0

    dest = Path(args.output) if args.output else default_output(src)
    try:
        atomic_write(dest, result.data, force=args.force)
    except SafetyError as exc:
        print(f"refused: {exc}", file=sys.stderr)
        return 1

    print(f"{src} -> {dest}  [mode: {args.mode}]")
    print("removed:")
    for item in result.removed:
        print(f"  {item}")
    print("preserved:")
    for item in result.preserved:
        print(f"  {item}")

    # Re-inspect. Claims are made only about what was actually re-checked.
    print("verification:")
    try:
        after = clean_bytes(dest, sniff(dest), args.mode)
        if after.changed:
            print("  WARNING: re-inspection still reports removable metadata")
        else:
            print("  re-inspected output: no removable provenance metadata remains")
    except SafetyError as exc:
        print(f"  could not re-inspect output: {exc}")

    if file_type in {"png", "jpeg"}:
        state, detail = c2pa_state(dest)
        if state == "absent":
            print(f"  c2patool: no embedded manifest remains ({detail})")
        elif state == "present":
            print("  WARNING: c2patool still reports an embedded manifest")
        else:
            print(f"  c2patool unavailable: C2PA state is unknown, not absent ({detail})")
        # The soft-binding caveat must fire whenever a C2PA carrier was actually
        # removed, whatever c2patool's availability. Our own parsers detect the
        # caBX chunk and APP11 segment directly, so gating this on the optional
        # tool would drop the caveat exactly when the tool is missing.
        removed_c2pa = any("C2PA" in r or "JUMBF" in r for r in result.removed)
        print("not verified:")
        if removed_c2pa or before_c2pa[0] == "present":
            print("  a C2PA soft binding, if the generator applied one, survives metadata removal")
            print("  and can re-link this asset to its manifest. This tool cannot detect one.")
        print("  pixel-domain watermarks (SynthID-class). No independent detector is available.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
