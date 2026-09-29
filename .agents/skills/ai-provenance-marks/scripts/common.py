"""Shared safety primitives for AI provenance inspection and cleaning.

Standard library only. No network. Nothing here mutates a file in place.

Design notes tied to observed failures in prior art
(guillaumemeyer/watermarks-remover @ 256d90d1, MIT):
  - Type detection is by magic bytes, never by extension, so a renamed binary
    cannot reach a text code path.
  - Tool output is parsed, never substring-matched. A missing tool yields
    "unknown", never "absent".
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field, asdict
from pathlib import Path

# Refuse absurd inputs rather than exhausting memory.
MAX_BYTES = 128 * 1024 * 1024

# Confidence tiers. Anything a raw byte scan produced is never "confirmed".
CONFIRMED = "confirmed"
PROBABLE = "probable"
INFORMATIONAL = "informational"

TEXT_TYPES = {"text", "markdown", "html", "svg", "code"}
BINARY_TYPES = {"png", "jpeg", "pdf", "docx", "odt", "zip", "unknown-binary"}


class SafetyError(Exception):
    """Raised when an operation would be unsafe. Callers must fail closed."""


@dataclass
class Finding:
    kind: str
    detail: str
    confidence: str
    count: int = 1
    removable: bool = True
    protected: bool = False
    note: str = ""


@dataclass
class Report:
    path: str
    file_type: str
    findings: list = field(default_factory=list)
    warnings: list = field(default_factory=list)
    unavailable: list = field(default_factory=list)

    def add(self, f: Finding) -> None:
        self.findings.append(f)

    def to_dict(self) -> dict:
        return {
            "path": self.path,
            "file_type": self.file_type,
            "findings": [asdict(f) for f in self.findings],
            "warnings": self.warnings,
            "unavailable": self.unavailable,
        }


# --------------------------------------------------------------- type sniffing

_MAGIC = [
    (b"\x89PNG\r\n\x1a\n", "png"),
    (b"\xff\xd8\xff", "jpeg"),
    (b"%PDF-", "pdf"),
    (b"GIF87a", "gif"),
    (b"GIF89a", "gif"),
    (b"RIFF", "riff"),
    (b"\x1f\x8b", "gzip"),
    (b"BM", "bmp"),
]


def sniff(path: Path) -> str:
    """Identify a file by content. Extension is only a tie-breaker for text."""
    size = path.stat().st_size
    if size > MAX_BYTES:
        raise SafetyError(f"{path}: {size} bytes exceeds the {MAX_BYTES} byte limit")
    with path.open("rb") as fh:
        head = fh.read(8192)

    for sig, name in _MAGIC:
        if head.startswith(sig):
            return name

    if head.startswith(b"PK\x03\x04"):
        return _sniff_zip(path)

    if _looks_binary(head):
        return "unknown-binary"

    suffix = path.suffix.lower()
    if suffix in {".md", ".markdown"}:
        return "markdown"
    if suffix in {".html", ".htm"}:
        return "html"
    if suffix == ".svg":
        return "svg"
    if suffix in {".py", ".js", ".mjs", ".ts", ".tsx", ".go", ".rs", ".java", ".c", ".h", ".sh"}:
        return "code"
    # An SVG or HTML file can arrive with any name; look at the content.
    sample = head.lstrip()[:512].lower()
    if sample.startswith(b"<?xml") and b"<svg" in head[:4096].lower():
        return "svg"
    if sample.startswith(b"<!doctype html") or sample.startswith(b"<html"):
        return "html"
    return "text"


def _sniff_zip(path: Path) -> str:
    import zipfile

    try:
        with zipfile.ZipFile(path) as zf:
            names = set(zf.namelist())
    except zipfile.BadZipFile as exc:
        raise SafetyError(f"{path}: not a readable zip container ({exc})") from exc
    if "word/document.xml" in names:
        return "docx"
    if "content.xml" in names and "meta.xml" in names:
        return "odt"
    return "zip"


def _looks_binary(head: bytes) -> bool:
    if b"\x00" in head:
        return True
    try:
        head.decode("utf-8")
    except UnicodeDecodeError:
        # A multi-byte sequence may be cut by the read boundary; retry shorter.
        try:
            head[:-4].decode("utf-8")
        except UnicodeDecodeError:
            return True
    return False


def require_text(path: Path, file_type: str) -> None:
    """Guard for text-only tools. Binary must never reach a text transform."""
    if file_type not in TEXT_TYPES:
        raise SafetyError(
            f"{path}: detected as '{file_type}', which is not a text format. "
            f"Text cleaning would corrupt it. Use the container cleaner instead."
        )


# ------------------------------------------------------------------ safe write


def default_output(src: Path) -> Path:
    """input.ext -> input.cleaned.ext"""
    return src.with_suffix(f".cleaned{src.suffix}") if src.suffix else src.with_name(src.name + ".cleaned")


def atomic_write(dest: Path, data: bytes, *, force: bool = False) -> None:
    """Write via a temp file in the destination directory, then replace.

    Never leaves a partial file at dest. Refuses to clobber unless forced.
    """
    if dest.exists() and not force:
        raise SafetyError(f"{dest}: already exists. Choose another path or pass --force")
    dest.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=str(dest.parent), prefix=".tmp-provenance-")
    tmp_path = Path(tmp)
    try:
        with os.fdopen(fd, "wb") as fh:
            fh.write(data)
            fh.flush()
            os.fsync(fh.fileno())
        if dest.exists():
            shutil.copymode(dest, tmp_path)
        os.replace(tmp_path, dest)
    except BaseException:
        tmp_path.unlink(missing_ok=True)
        raise


# --------------------------------------------------------------- optional tools


def tool_available(name: str) -> bool:
    return shutil.which(name) is not None


def run_tool(argv: list[str], *, timeout: int = 60) -> tuple[int, str, str]:
    """Run an external tool with no shell. Returns (code, stdout, stderr)."""
    try:
        proc = subprocess.run(
            argv, capture_output=True, text=True, timeout=timeout, shell=False, check=False
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise SafetyError(f"failed to run {argv[0]}: {exc}") from exc
    return proc.returncode, proc.stdout, proc.stderr


def c2pa_state(path: Path) -> tuple[str, str]:
    """Return (state, detail) where state is present | absent | unknown.

    'unknown' when c2patool is missing. Never guess from EXIF, and never
    substring-match the tool's prose: prior art reported a manifest on every
    file because "No claim found" contains the word "claim".
    """
    if not tool_available("c2patool"):
        return "unknown", "c2patool not installed; C2PA state cannot be determined"
    code, out, err = run_tool(["c2patool", str(path), "--detailed"])
    if code == 0 and out.strip():
        try:
            parsed = json.loads(out)
        except json.JSONDecodeError:
            return "unknown", "c2patool output was not valid JSON"
        if isinstance(parsed, dict) and (parsed.get("manifests") or parsed.get("active_manifest")):
            return "present", "c2patool reported an embedded manifest"
        return "absent", "c2patool parsed the asset and reported no manifest"
    combined = (err or out).strip()
    if "no claim found" in combined.lower() or "jumbf not found" in combined.lower():
        return "absent", "c2patool reported no claim"
    return "unknown", f"c2patool exited {code}: {combined[:200]}"


# ------------------------------------------------------------------------ cli


def emit(report_obj: dict, as_json: bool) -> None:
    if as_json:
        json.dump(report_obj, sys.stdout, indent=2, ensure_ascii=False)
        sys.stdout.write("\n")
    else:
        _human(report_obj)


def _human(r: dict) -> None:
    print(f"{r['path']}  [{r['file_type']}]")
    findings = r.get("findings", [])
    if not findings:
        print("  no supported AI provenance marks detected")
    for f in findings:
        flag = " (protected, not removed by default)" if f.get("protected") else ""
        count = f" x{f['count']}" if f.get("count", 1) > 1 else ""
        print(f"  [{f['confidence']}] {f['kind']}: {f['detail']}{count}{flag}")
        if f.get("note"):
            print(f"      {f['note']}")
    for w in r.get("warnings", []):
        print(f"  warning: {w}")
    for u in r.get("unavailable", []):
        print(f"  unverified: {u}")
