"""Container metadata inspection and removal.

Two modes, and the distinction is deliberate:

  provenance    (default) remove AI provenance carriers only: C2PA manifests,
                XMP packets, AI generator tags. EXIF is preserved, so camera
                orientation, colour and timestamps survive.
  all-metadata  remove metadata broadly, including EXIF. Never the default,
                because "strip the AI provenance" does not authorise deleting a
                photograph's orientation tag.

Encoded pixel payloads are never re-encoded. PNG IDAT chunks and JPEG entropy
data are copied byte for byte, so a metadata clean cannot alter the image.
"""

from __future__ import annotations

import io
import re
import struct
import zipfile
from dataclasses import dataclass
from pathlib import Path

from common import CONFIRMED, INFORMATIONAL, PROBABLE, Finding, SafetyError

# Markers that indicate AI provenance inside a metadata value. Matched only
# against metadata, never against visible document text.
#
# Two tiers, because a flat substring list produces false positives on ordinary
# content: "imagen de portada" is Spanish for cover image, "flux" is a common
# library name, and "Claude Monet" is a painter. Weak markers therefore only
# count when a generation context accompanies them, or when the value is
# essentially just the marker.
STRONG_MARKERS = (
    "c2pa", "contentcredentials", "content credentials", "synthid",
    "openai", "dall-e", "dall·e", "midjourney", "stable diffusion",
    "stablediffusion", "generativeai", "generative ai",
    "ai-generated", "aigenerated", "anthropic",
)
WEAK_MARKERS = ("claude", "gemini", "imagen", "grok", "flux", "firefly")
GENERATION_CONTEXT = (
    "generated", "generator", "created with", "created by", "made with",
    "produced by", "model", "prompt", "assistant", "diffusion", "ai",
)

# Metadata keys that describe the producing tool.
GENERATOR_KEYS = (
    "software", "creatortool", "generator", "application", "producer",
    "parameters", "prompt", "workflow", "xmp", "createdby", "creator-tool",
)

# Frontmatter keys removed in provenance mode. Everything else is preserved.
# Unambiguous: the key itself means AI provenance whatever its value.
FRONTMATTER_PROVENANCE_KEYS = {
    "ai", "ai_generated", "ai-generated", "aigenerated", "generated_by",
    "generated-by", "generatedby", "c2pa", "content_credentials",
    "content-credentials", "provenance", "synthid",
}
# Ambiguous: legitimate in ordinary front matter. A static site sets
# "generator: Docusaurus"; a data file sets "model: gpt-4" or "model: Ford".
# These are removed only when the value itself is an AI provenance marker.
FRONTMATTER_AMBIGUOUS_KEYS = {"generator", "model", "llm", "assistant"}


@dataclass
class CleanResult:
    data: bytes
    removed: list
    preserved: list
    changed: bool


def _word_present(needle: str, haystack: str) -> bool:
    """Word-boundary match, so 'flux' does not fire inside 'influx'."""
    return re.search(rf"(?<![a-z0-9]){re.escape(needle)}(?![a-z0-9])", haystack) is not None


def _value_is_ai(value: str) -> bool:
    low = value.lower()
    if any(_word_present(m, low) for m in STRONG_MARKERS):
        return True
    for m in WEAK_MARKERS:
        if not _word_present(m, low):
            continue
        # Accompanied by a generation context, or the value is just the marker
        # and perhaps a version, as in "Claude" or "Gemini 2.0".
        if any(_word_present(c, low) for c in GENERATION_CONTEXT):
            return True
        if re.fullmatch(rf"\s*{re.escape(m)}[\s\-_/v.0-9]*", low):
            return True
    return False


# ------------------------------------------------------------------------ PNG

_PNG_SIG = b"\x89PNG\r\n\x1a\n"
_PNG_RENDER_KEEP = {b"tRNS", b"gAMA", b"cHRM", b"sRGB", b"iCCP", b"pHYs", b"sBIT", b"bKGD", b"hIST", b"acTL", b"fcTL", b"fdAT"}
_PNG_TEXT = {b"tEXt", b"iTXt", b"zTXt"}
_PNG_C2PA = {b"caBX"}
_PNG_EXIF = {b"eXIf"}


def _png_chunks(data: bytes):
    if not data.startswith(_PNG_SIG):
        raise SafetyError("not a PNG file")
    pos = len(_PNG_SIG)
    while pos < len(data):
        if pos + 8 > len(data):
            raise SafetyError("truncated PNG: incomplete chunk header")
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        ctype = data[pos + 4 : pos + 8]
        end = pos + 12 + length
        if length > len(data) or end > len(data):
            raise SafetyError("truncated PNG: chunk length exceeds file size")
        body = data[pos + 8 : pos + 8 + length]
        yield ctype, body, data[pos:end]
        pos = end
    return


def png_inspect(data: bytes) -> list[Finding]:
    out: list[Finding] = []
    saw_iend = False
    for ctype, body, _raw in _png_chunks(data):
        if ctype == b"IEND":
            saw_iend = True
        if ctype in _PNG_C2PA:
            out.append(Finding("c2pa", "embedded C2PA manifest chunk (caBX)", CONFIRMED))
        elif ctype in _PNG_EXIF:
            out.append(Finding("exif", "eXIf chunk", INFORMATIONAL, removable=False,
                               note="preserved in provenance mode"))
        elif ctype in _PNG_TEXT:
            text = body.decode("latin-1", "replace")
            keyword = text.split("\x00", 1)[0].lower()
            if "adobe.xmp" in text.lower() or keyword == "xml:com.adobe.xmp":
                conf = CONFIRMED if _value_is_ai(text) else PROBABLE
                out.append(Finding("xmp", f"XMP packet in {ctype.decode()} chunk", conf))
            elif _value_is_ai(text):
                out.append(Finding("generator", f"AI marker in {ctype.decode()} '{keyword}'", PROBABLE))
            elif keyword in GENERATOR_KEYS:
                out.append(Finding("generator", f"{ctype.decode()} '{keyword}'", INFORMATIONAL))
    if not saw_iend:
        raise SafetyError("truncated PNG: no IEND chunk")
    return out


def png_clean(data: bytes, mode: str) -> CleanResult:
    kept: list[bytes] = [_PNG_SIG]
    removed: list[str] = []
    preserved: list[str] = []
    for ctype, body, raw in _png_chunks(data):
        name = ctype.decode("latin-1", "replace")
        # PNG encodes criticality in the case of the first letter: uppercase
        # means a decoder must not skip it. Never delete one, whatever the mode.
        # A private critical chunk we do not recognise is still required.
        is_critical = ctype[:1].isupper()
        if is_critical or ctype in _PNG_RENDER_KEEP:
            kept.append(raw)
            continue
        drop = False
        if ctype in _PNG_C2PA:
            drop, why = True, "C2PA manifest chunk (caBX)"
        elif ctype in _PNG_TEXT:
            text = body.decode("latin-1", "replace")
            keyword = text.split("\x00", 1)[0]
            if "adobe.xmp" in text.lower():
                drop, why = True, f"XMP packet in {name}"
            elif _value_is_ai(text):
                drop, why = True, f"AI marker in {name} '{keyword}'"
            elif mode == "all-metadata":
                drop, why = True, f"{name} '{keyword}'"
        elif ctype in _PNG_EXIF:
            if mode == "all-metadata":
                drop, why = True, "eXIf chunk"
            else:
                preserved.append("EXIF (eXIf chunk)")
        elif mode == "all-metadata":
            drop, why = True, f"ancillary chunk {name}"

        if drop:
            removed.append(why)
        else:
            kept.append(raw)
    preserved.append("pixel data (IDAT chunks copied unchanged)")
    return CleanResult(b"".join(kept), removed, preserved, bool(removed))


# ----------------------------------------------------------------------- JPEG

_EXIF_SIG = b"Exif\x00\x00"
_XMP_SIG = b"http://ns.adobe.com/xap/1.0/\x00"
_XMP_EXT_SIG = b"http://ns.adobe.com/xmp/extension/\x00"


def _jpeg_segments(data: bytes):
    if not data.startswith(b"\xff\xd8"):
        raise SafetyError("not a JPEG file")
    pos = 2
    yield b"\xff\xd8", b"", data[0:2]
    while pos < len(data):
        if data[pos] != 0xFF:
            raise SafetyError(f"malformed JPEG: expected marker at offset {pos}")
        marker = data[pos : pos + 2]
        m = data[pos + 1]
        if m == 0xD9:  # EOI
            yield marker, b"", data[pos : pos + 2]
            return
        if m == 0xDA:  # SOS: entropy-coded data runs to EOI
            yield marker, data[pos + 2 :], data[pos:]
            return
        if pos + 4 > len(data):
            raise SafetyError("truncated JPEG: incomplete segment header")
        (length,) = struct.unpack(">H", data[pos + 2 : pos + 4])
        end = pos + 2 + length
        if length < 2 or end > len(data):
            raise SafetyError("truncated JPEG: segment length exceeds file size")
        yield marker, data[pos + 4 : end], data[pos:end]
        pos = end
    raise SafetyError("truncated JPEG: no end-of-image marker")


def jpeg_inspect(data: bytes) -> list[Finding]:
    out: list[Finding] = []
    for marker, body, _raw in _jpeg_segments(data):
        m = marker[1] if len(marker) > 1 else 0
        if m == 0xEB:  # APP11, JUMBF / C2PA
            out.append(Finding("c2pa", "APP11 JUMBF segment (C2PA carrier)", CONFIRMED))
        elif m == 0xE1:
            if body.startswith(_XMP_SIG) or body.startswith(_XMP_EXT_SIG):
                conf = CONFIRMED if _value_is_ai(body.decode("latin-1", "replace")) else PROBABLE
                out.append(Finding("xmp", "APP1 XMP packet", conf))
            elif body.startswith(_EXIF_SIG):
                out.append(Finding("exif", "APP1 EXIF segment", INFORMATIONAL, removable=False,
                                   note="preserved in provenance mode; contains orientation"))
        elif m == 0xFE:
            text = body.decode("latin-1", "replace")
            if _value_is_ai(text):
                out.append(Finding("generator", "AI marker in JPEG comment", PROBABLE))
    return out


def jpeg_clean(data: bytes, mode: str) -> CleanResult:
    kept: list[bytes] = []
    removed: list[str] = []
    preserved: list[str] = []
    for marker, body, raw in _jpeg_segments(data):
        m = marker[1] if len(marker) > 1 else 0
        drop = False
        why = ""
        if m == 0xEB:
            drop, why = True, "APP11 JUMBF segment (C2PA carrier)"
        elif m == 0xE1:
            if body.startswith(_XMP_SIG) or body.startswith(_XMP_EXT_SIG):
                drop, why = True, "APP1 XMP packet"
            elif body.startswith(_EXIF_SIG):
                if mode == "all-metadata":
                    drop, why = True, "APP1 EXIF segment"
                else:
                    preserved.append("EXIF (APP1, includes orientation)")
        elif m == 0xFE:
            text = body.decode("latin-1", "replace")
            if _value_is_ai(text) or mode == "all-metadata":
                drop, why = True, "JPEG comment segment"
        if drop:
            removed.append(why)
        else:
            kept.append(raw)
    preserved.append("entropy-coded scan data (copied unchanged)")
    return CleanResult(b"".join(kept), removed, preserved, bool(removed))


# ------------------------------------------------------------------ SVG / XML

_DOCTYPE_ENTITY = re.compile(rb"<!DOCTYPE[^>]*\[[^\]]*<!ENTITY", re.IGNORECASE | re.DOTALL)


def _guard_xml(data: bytes) -> None:
    """Refuse XML that declares its own entities.

    Recent CPython's expat caps entity amplification and does not resolve
    external entities, but that guard is version dependent and this skill
    advertises Python 3.9+. An explicit check makes the behaviour the same on
    every interpreter. Expanding entities would also rewrite content that has
    nothing to do with provenance, which the minimal-change rule forbids.
    Legitimate SVG does not need custom entity declarations.
    """
    if _DOCTYPE_ENTITY.search(data):
        raise SafetyError(
            "XML declares entities in its DOCTYPE. Refusing: entity expansion can "
            "amplify input and would rewrite content unrelated to provenance"
        )


def svg_inspect(data: bytes) -> list[Finding]:
    import xml.etree.ElementTree as ET

    _guard_xml(data)
    try:
        root = ET.fromstring(data.decode("utf-8", "replace"))
    except ET.ParseError as exc:
        raise SafetyError(f"malformed SVG/XML: {exc}") from exc
    out: list[Finding] = []
    for el in root.iter():
        tag = el.tag.split("}")[-1].lower()
        if tag in {"metadata", "rdf"}:
            blob = ET.tostring(el, encoding="unicode")
            conf = CONFIRMED if _value_is_ai(blob) else PROBABLE
            out.append(Finding("xmp", f"<{tag}> block", conf))
    return out


def svg_clean(data: bytes, mode: str) -> CleanResult:
    import xml.etree.ElementTree as ET

    _guard_xml(data)
    text = data.decode("utf-8", "replace")
    try:
        root = ET.fromstring(text)
    except ET.ParseError as exc:
        raise SafetyError(f"malformed SVG/XML: {exc}") from exc

    # Re-register the source's own namespace prefixes before serialising.
    # Without this, ElementTree invents ns0: prefixes and rewrites every tag in
    # the document, which is a content change we did not ask for.
    for prefix, uri in re.findall(r'xmlns:([A-Za-z0-9_.-]+)\s*=\s*"([^"]+)"', text):
        ET.register_namespace(prefix, uri)
    default_ns = re.search(r'xmlns\s*=\s*"([^"]+)"', text)
    if default_ns:
        ET.register_namespace("", default_ns.group(1))

    removed: list[str] = []

    def strip(parent) -> None:
        for child in list(parent):
            tag = child.tag.split("}")[-1].lower()
            if tag in {"metadata", "rdf"}:
                parent.remove(child)
                removed.append(f"<{tag}> block")
            else:
                strip(child)

    strip(root)
    out = ET.tostring(root, encoding="utf-8", xml_declaration=text.lstrip().startswith("<?xml"))
    return CleanResult(out, removed, ["all drawing elements"], bool(removed))


# ----------------------------------------------------------------------- HTML


class _MetaScanner:
    """Locate provenance <meta> tags by parsing, not by regex over markup."""

    def __init__(self, text: str):
        from html.parser import HTMLParser

        hits: list[tuple[int, int, str]] = []
        src = text

        class P(HTMLParser):
            def handle_starttag(self, tag, attrs):
                if tag not in {"meta", "script"}:
                    return
                a = {k.lower(): (v or "") for k, v in attrs}
                label = None
                if tag == "meta":
                    key = (a.get("name") or a.get("property") or "").lower()
                    val = a.get("content", "")
                    if key in {"generator", "ai", "ai-generated", "author"} and _value_is_ai(val):
                        label = f"<meta {key}=\"{val[:60]}\">"
                    elif key in {"generator"} and val:
                        label = f"<meta generator=\"{val[:60]}\">"
                elif tag == "script" and a.get("type", "").lower() == "application/ld+json":
                    label = "<script type=application/ld+json>"
                if label is None:
                    return
                start = self.getpos()
                hits.append((start[0], start[1], label))

        parser = P(convert_charrefs=True)
        parser.feed(src)
        parser.close()
        self.hits = hits


def html_inspect(data: bytes) -> list[Finding]:
    text = data.decode("utf-8", "replace")
    out: list[Finding] = []
    for _line, _col, label in _MetaScanner(text).hits:
        if label.startswith("<script"):
            out.append(Finding("jsonld", "JSON-LD block", INFORMATIONAL, removable=False,
                               note="inspected only; not removed automatically"))
        else:
            conf = CONFIRMED if _value_is_ai(label) else INFORMATIONAL
            out.append(Finding("generator", label, conf, removable=_value_is_ai(label)))
    return out


def html_clean(data: bytes, mode: str) -> CleanResult:
    """Remove only whole <meta> tags whose content is an AI provenance marker.

    Operates on exact tag spans located by the parser, so surrounding markup and
    all body content are untouched.
    """
    text = data.decode("utf-8", "replace")
    removed: list[str] = []
    pattern = re.compile(r"<meta\b[^>]*>", re.IGNORECASE)
    out_parts: list[str] = []
    last = 0
    for m in pattern.finditer(text):
        tag = m.group(0)
        attrs = dict(re.findall(r'([a-zA-Z-]+)\s*=\s*"([^"]*)"', tag))
        key = (attrs.get("name") or attrs.get("property") or "").lower()
        val = attrs.get("content", "")
        drop = (key in {"generator", "ai", "ai-generated"} and _value_is_ai(val)) or (
            mode == "all-metadata" and key == "generator"
        )
        if drop:
            out_parts.append(text[last : m.start()])
            last = m.end()
            # Swallow a trailing newline left by the removed tag.
            if text[last : last + 1] == "\n":
                last += 1
            removed.append(f"<meta {key}=\"{val[:60]}\">")
    out_parts.append(text[last:])
    return CleanResult("".join(out_parts).encode("utf-8"), removed,
                       ["document body and all other markup"], bool(removed))


# ------------------------------------------------------------------- Markdown

_FM_RE = re.compile(r"\A---\r?\n(.*?)\r?\n---\r?\n", re.DOTALL)


def markdown_inspect(data: bytes) -> list[Finding]:
    text = data.decode("utf-8", "replace")
    m = _FM_RE.match(text)
    if not m:
        return []
    out: list[Finding] = []
    for line in m.group(1).splitlines():
        if ":" not in line or line.startswith((" ", "\t", "#")):
            continue
        key = line.split(":", 1)[0].strip().lower()
        value = line.split(":", 1)[1].strip()
        if key in FRONTMATTER_PROVENANCE_KEYS:
            out.append(Finding("frontmatter", f"provenance key '{key}'", CONFIRMED))
        elif key in FRONTMATTER_AMBIGUOUS_KEYS and _value_is_ai(value):
            out.append(Finding("frontmatter", f"AI value in '{key}'", CONFIRMED))
        elif _value_is_ai(value):
            out.append(Finding("frontmatter", f"AI marker in '{key}'", PROBABLE))
    return out


def markdown_clean(data: bytes, mode: str) -> CleanResult:
    text = data.decode("utf-8", "replace")
    m = _FM_RE.match(text)
    if not m:
        return CleanResult(data, [], ["entire document"], False)

    removed: list[str] = []
    kept_lines: list[str] = []
    skipping = False
    for line in m.group(1).splitlines():
        is_top_level = bool(line) and not line[0].isspace() and ":" in line
        if is_top_level:
            key = line.split(":", 1)[0].strip().lower()
            value = line.split(":", 1)[1].strip()
            # Unambiguous key, or any key whose value is itself a marker.
            # "generator: Docusaurus" satisfies neither and is preserved.
            if key in FRONTMATTER_PROVENANCE_KEYS or _value_is_ai(value):
                removed.append(f"frontmatter key '{key}'")
                skipping = True
                continue
            skipping = False
        elif skipping:
            # Continuation of a removed block mapping or list.
            continue
        kept_lines.append(line)

    if not removed:
        return CleanResult(data, [], ["entire document"], False)

    body = text[m.end() :]
    if [line for line in kept_lines if line.strip()]:
        new = "---\n" + "\n".join(kept_lines).strip("\n") + "\n---\n" + body
    else:
        new = body
        removed.append("empty frontmatter block")
    return CleanResult(new.encode("utf-8"), removed, ["all Markdown body content"], True)


# --------------------------------------------------------------- DOCX and ODT

_DOCX_META = ("docProps/core.xml", "docProps/app.xml", "docProps/custom.xml")
_ODT_META = ("meta.xml",)


# Guards for untrusted archives: a DOCX is attacker-controllable input.
MAX_ZIP_UNCOMPRESSED = 256 * 1024 * 1024
MAX_ZIP_RATIO = 200


def _check_archive(zf: zipfile.ZipFile, path: Path) -> None:
    """Refuse traversal entries and decompression bombs before reading anything."""
    total = 0
    for info in zf.infolist():
        name = info.filename
        if name.startswith("/") or ".." in Path(name).parts or (len(name) > 1 and name[1] == ":"):
            raise SafetyError(f"{path}: archive entry escapes the container: {name!r}")
        total += info.file_size
        if total > MAX_ZIP_UNCOMPRESSED:
            raise SafetyError(
                f"{path}: archive expands to more than {MAX_ZIP_UNCOMPRESSED} bytes; refusing")
        if info.compress_size and info.file_size / info.compress_size > MAX_ZIP_RATIO:
            raise SafetyError(
                f"{path}: entry {name!r} has a compression ratio above {MAX_ZIP_RATIO}:1; refusing")


def _zip_names(path: Path) -> list[str]:
    with zipfile.ZipFile(path) as zf:
        bad = zf.testzip()
        if bad is not None:
            raise SafetyError(f"corrupt entry in archive: {bad}")
        return zf.namelist()


def office_inspect(path: Path, kind: str) -> list[Finding]:
    targets = _DOCX_META if kind == "docx" else _ODT_META
    out: list[Finding] = []
    with zipfile.ZipFile(path) as zf:
        _check_archive(zf, path)
        names = zf.namelist()
        for name in targets:
            if name not in names:
                continue
            blob = zf.read(name).decode("utf-8", "replace")
            if _value_is_ai(blob):
                out.append(Finding("generator", f"AI marker in {name}", PROBABLE))
            else:
                out.append(Finding("properties", f"{name} present", INFORMATIONAL,
                                   removable=False, note="document properties; removed only in all-metadata mode"))
    return out


def office_clean(path: Path, kind: str, mode: str) -> CleanResult:
    """Rewrite the archive, scrubbing property parts. Content parts are copied.

    Only known property parts are touched. Arbitrary customXml is left alone,
    because its meaning is application-defined and deleting it can break a
    document's content controls.
    """
    targets = set(_DOCX_META if kind == "docx" else _ODT_META)
    removed: list[str] = []
    buf = io.BytesIO()

    with zipfile.ZipFile(path) as src:
        _check_archive(src, path)
        infos = src.infolist()
        with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as dst:
            for info in infos:
                blob = src.read(info.filename)
                if info.filename in targets:
                    text = blob.decode("utf-8", "replace")
                    if _value_is_ai(text) or mode == "all-metadata":
                        new_text = _scrub_props(text, mode)
                        if new_text != text:
                            removed.append(f"provenance values in {info.filename}")
                            blob = new_text.encode("utf-8")
                dst.writestr(info, blob)
    return CleanResult(buf.getvalue(), removed, ["document body and all content parts"], bool(removed))


_PROP_TAGS = (
    "Application", "AppVersion", "Company", "Manager", "Template",
    "dc:creator", "cp:lastModifiedBy", "meta:generator", "meta:initial-creator",
)


def _scrub_props(text: str, mode: str) -> str:
    out = text
    for tag in _PROP_TAGS:
        pattern = re.compile(rf"<{re.escape(tag)}>(.*?)</{re.escape(tag)}>", re.DOTALL)

        def repl(m: re.Match) -> str:
            value = m.group(1)
            if mode == "all-metadata" or _value_is_ai(value):
                return f"<{tag}></{tag}>"
            return m.group(0)

        out = pattern.sub(repl, out)
    return out
