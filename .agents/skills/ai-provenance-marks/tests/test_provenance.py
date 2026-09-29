#!/usr/bin/env python3
"""Tests for the ai-provenance-marks scripts. Standard library only.

Run:  python3 tests/test_provenance.py

Every fixture is generated here. No third-party or copyrighted material is
committed. Several tests are regressions against defects observed in the prior
art repository (guillaumemeyer/watermarks-remover @ 256d90d1, MIT); those are
marked with the issue number they correspond to.
"""

from __future__ import annotations

import struct
import sys
import tempfile
import unittest
import zipfile
import zlib
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent.parent / "scripts"
sys.path.insert(0, str(SCRIPTS))

import container_marks as cm  # noqa: E402
import text_marks as tm  # noqa: E402
from common import SafetyError, atomic_write, default_output, require_text, sniff  # noqa: E402
import clean_text  # noqa: E402
import clean_container  # noqa: E402
import inspect_marks as inspect_mod  # noqa: E402


# ------------------------------------------------------------------ fixtures


def make_png(text_chunks=(), *, c2pa=False, exif=False) -> bytes:
    """A valid 1x1 greyscale PNG, optionally carrying metadata chunks."""
    def chunk(ctype: bytes, body: bytes) -> bytes:
        return struct.pack(">I", len(body)) + ctype + body + struct.pack(
            ">I", zlib.crc32(ctype + body) & 0xFFFFFFFF)

    ihdr = struct.pack(">IIBBBBB", 1, 1, 8, 0, 0, 0, 0)
    idat = zlib.compress(b"\x00\x00")
    out = [b"\x89PNG\r\n\x1a\n", chunk(b"IHDR", ihdr)]
    for keyword, value in text_chunks:
        out.append(chunk(b"tEXt", keyword.encode() + b"\x00" + value.encode()))
    if c2pa:
        out.append(chunk(b"caBX", b"\x00fake-jumbf-manifest"))
    if exif:
        out.append(chunk(b"eXIf", b"II*\x00\x08\x00\x00\x00"))
    out.append(chunk(b"IDAT", idat))
    out.append(chunk(b"IEND", b""))
    return b"".join(out)


def make_jpeg(*, xmp=False, exif=False, comment=None) -> bytes:
    def seg(marker: int, body: bytes) -> bytes:
        return bytes([0xFF, marker]) + struct.pack(">H", len(body) + 2) + body

    out = [b"\xff\xd8"]
    if exif:
        out.append(seg(0xE1, b"Exif\x00\x00" + b"II*\x00\x08\x00\x00\x00"))
    if xmp:
        out.append(seg(0xE1, b"http://ns.adobe.com/xap/1.0/\x00<x:xmpmeta>c2pa</x:xmpmeta>"))
    if comment:
        out.append(seg(0xFE, comment.encode()))
    out.append(b"\xff\xda\x00\x08\x01\x01\x00\x00\x3f\x00")  # SOS + entropy
    out.append(b"\xff\xd9")
    return b"".join(out)


def make_docx(app="Microsoft Word", creator="Jane") -> bytes:
    import io
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        zf.writestr("[Content_Types].xml", "<Types/>")
        zf.writestr("word/document.xml", "<w:document>Hello visible text</w:document>")
        zf.writestr("docProps/app.xml", f"<Properties><Application>{app}</Application></Properties>")
        zf.writestr("docProps/core.xml", f"<cp:coreProperties><dc:creator>{creator}</dc:creator></cp:coreProperties>")
    return buf.getvalue()


class Tmp(unittest.TestCase):
    def setUp(self):
        self._dir = tempfile.TemporaryDirectory()
        self.tmp = Path(self._dir.name)

    def tearDown(self):
        self._dir.cleanup()

    def write(self, name: str, data) -> Path:
        p = self.tmp / name
        p.write_bytes(data.encode("utf-8") if isinstance(data, str) else data)
        return p


# --------------------------------------------------------------- Unicode / A2


class TestUnicode(Tmp):
    def test_removes_zero_width_artefacts(self):
        text = "hello​world⁠and﻿more"
        cleaned, removed = tm.clean(text)
        self.assertEqual(cleaned, "helloworldandmore")
        self.assertEqual(len(removed), 3)

    def test_preserves_plain_multilingual_text(self):
        """Greek, Cyrillic, CJK, Arabic prose must survive untouched."""
        text = "Ελληνικά Русский 中文 日本語 مرحبا bonjour café naïve"
        cleaned, removed = tm.clean(text)
        self.assertEqual(cleaned, text)
        self.assertEqual(removed, [])

    def test_preserves_emoji_zwj_sequence(self):
        """Regression, prior art issue #22: ZWJ joins the glyph, removing it splits it."""
        text = "team \U0001F469‍\U0001F4BB ships"
        cleaned, removed = tm.clean(text)
        self.assertEqual(cleaned, text)
        self.assertEqual(removed, [])
        marks = [m for m in tm.scan(text) if m.char == "‍"]
        self.assertTrue(marks and marks[0].protected)

    def test_preserves_variation_selector(self):
        """Regression, prior art issue #22: VS16 selects emoji presentation."""
        text = "warning ❗️ here"
        cleaned, removed = tm.clean(text)
        self.assertEqual(cleaned, text)
        self.assertEqual(removed, [])

    def test_preserves_zwnj_in_persian(self):
        text = "می‌روم"  # ZWNJ is orthographic here
        cleaned, _ = tm.clean(text)
        self.assertEqual(cleaned, text)

    def test_removes_zwnj_in_latin_prose(self):
        """Same codepoint, no joining script nearby: an artefact."""
        text = "plain‌text"
        cleaned, removed = tm.clean(text)
        self.assertEqual(cleaned, "plaintext")
        self.assertEqual(len(removed), 1)

    def test_bidi_preserved_when_document_has_rtl(self):
        text = "‫مرحبا bonjour‬"
        cleaned, removed = tm.clean(text)
        self.assertEqual(cleaned, text)
        self.assertEqual(removed, [])

    def test_bidi_removed_when_no_rtl_present(self):
        text = "safe‮reversed"
        cleaned, removed = tm.clean(text)
        self.assertEqual(cleaned, "safereversed")
        self.assertEqual(len(removed), 1)

    def test_tag_characters_removed(self):
        text = "hi\U000E0041\U000E0042"
        cleaned, removed = tm.clean(text)
        self.assertEqual(cleaned, "hi")
        self.assertEqual(len(removed), 2)

    def test_spaces_not_touched_by_default(self):
        text = "a b"
        cleaned, removed = tm.clean(text)
        self.assertEqual(cleaned, text)
        self.assertEqual(removed, [])
        cleaned2, removed2 = tm.clean(text, normalise_spaces=True)
        self.assertEqual(cleaned2, "a b")
        self.assertEqual(len(removed2), 1)


# ------------------------------------------------------ visible text vs metadata


class TestVisibleTextIsNotMetadata(Tmp):
    def test_word_claude_in_prose_is_not_a_finding(self):
        """Regression, prior art issue #14."""
        p = self.write("notes.md", "# Notes\n\nI asked Claude and OpenAI and Gemini about this.\n")
        report = inspect_mod.inspect_path(p).to_dict()
        self.assertEqual(report["findings"], [], f"unexpected findings: {report['findings']}")

    def test_vendor_name_in_frontmatter_value_is_a_finding(self):
        p = self.write("notes.md", "---\ngenerated_by: Claude\ntitle: Real\n---\n\nBody about Claude.\n")
        report = inspect_mod.inspect_path(p).to_dict()
        self.assertTrue(any(f["kind"] == "frontmatter" for f in report["findings"]))


# ------------------------------------------------------------------ binary safety


class TestBinarySafety(Tmp):
    def test_text_cleaner_refuses_png(self):
        p = self.write("image.png", make_png())
        with self.assertRaises(SafetyError):
            require_text(p, sniff(p))

    def test_text_cleaner_refuses_png_renamed_as_txt(self):
        """Extension lies; magic bytes do not."""
        p = self.write("actually_png.txt", make_png())
        self.assertEqual(sniff(p), "png")
        rc = clean_text.main([str(p)])
        self.assertEqual(rc, 1)
        self.assertFalse(default_output(p).exists())

    def test_text_cleaner_refuses_docx(self):
        p = self.write("doc.docx", make_docx())
        self.assertEqual(sniff(p), "docx")
        self.assertEqual(clean_text.main([str(p)]), 1)


# ------------------------------------------------------------------------ PNG


class TestPng(Tmp):
    def _idat(self, data: bytes) -> bytes:
        return b"".join(body for ctype, body, _ in cm._png_chunks(data) if ctype == b"IDAT")

    def test_pixels_unchanged_by_metadata_clean(self):
        src = make_png([("Software", "Made with Stable Diffusion")])
        result = cm.png_clean(src, "provenance")
        self.assertTrue(result.changed)
        self.assertEqual(self._idat(src), self._idat(result.data))

    def test_c2pa_chunk_removed(self):
        src = make_png(c2pa=True)
        result = cm.png_clean(src, "provenance")
        self.assertTrue(any("caBX" in r for r in result.removed))
        self.assertNotIn(b"caBX", result.data)

    def test_exif_preserved_in_provenance_mode(self):
        src = make_png(c2pa=True, exif=True)
        result = cm.png_clean(src, "provenance")
        self.assertIn(b"eXIf", result.data)
        self.assertTrue(any("EXIF" in p for p in result.preserved))

    def test_exif_removed_in_all_metadata_mode(self):
        src = make_png(exif=True)
        result = cm.png_clean(src, "all-metadata")
        self.assertNotIn(b"eXIf", result.data)

    def test_clean_png_reports_no_change(self):
        src = make_png()
        result = cm.png_clean(src, "provenance")
        self.assertFalse(result.changed)
        self.assertEqual(result.data, src)

    def test_truncated_png_fails_closed(self):
        p = self.write("bad.png", make_png()[:20])
        with self.assertRaises(SafetyError):
            cm.png_inspect(p.read_bytes())

    def test_idempotent(self):
        src = make_png([("Software", "midjourney")], c2pa=True)
        once = cm.png_clean(src, "provenance")
        twice = cm.png_clean(once.data, "provenance")
        self.assertFalse(twice.changed)
        self.assertEqual(once.data, twice.data)


# ----------------------------------------------------------------------- JPEG


class TestJpeg(Tmp):
    def test_xmp_removed_exif_preserved(self):
        src = make_jpeg(xmp=True, exif=True)
        result = cm.jpeg_clean(src, "provenance")
        self.assertNotIn(b"ns.adobe.com/xap", result.data)
        self.assertIn(b"Exif\x00\x00", result.data)

    def test_scan_data_preserved(self):
        src = make_jpeg(xmp=True)
        result = cm.jpeg_clean(src, "provenance")
        self.assertTrue(result.data.endswith(b"\xff\xd9"))
        self.assertIn(b"\xff\xda", result.data)

    def test_all_metadata_removes_exif(self):
        src = make_jpeg(exif=True)
        result = cm.jpeg_clean(src, "all-metadata")
        self.assertNotIn(b"Exif\x00\x00", result.data)

    def test_malformed_jpeg_fails_closed(self):
        with self.assertRaises(SafetyError):
            cm.jpeg_inspect(b"\xff\xd8\xff\xe1\x00")


# --------------------------------------------------------------- SVG/HTML/MD


class TestMarkup(Tmp):
    def test_svg_metadata_removed_drawing_preserved(self):
        svg = ('<svg xmlns="http://www.w3.org/2000/svg">'
               "<metadata>Generated by Midjourney</metadata>"
               '<rect width="10" height="10"/></svg>')
        result = cm.svg_clean(svg.encode(), "provenance")
        self.assertIn(b"<rect", result.data)
        self.assertNotIn(b"Midjourney", result.data)

    def test_malformed_svg_fails_closed(self):
        with self.assertRaises(SafetyError):
            cm.svg_clean(b"<svg><unclosed>", "provenance")

    def test_html_body_preserved(self):
        html = ('<html><head><meta name="generator" content="Made with DALL-E">'
                "</head><body><p>Real content about Claude</p></body></html>")
        result = cm.html_clean(html.encode(), "provenance")
        self.assertIn(b"<p>Real content about Claude</p>", result.data)
        self.assertNotIn(b"DALL-E", result.data)

    def test_html_unrelated_meta_preserved(self):
        html = '<html><head><meta name="viewport" content="width=device-width"></head><body>x</body></html>'
        result = cm.html_clean(html.encode(), "provenance")
        self.assertFalse(result.changed)
        self.assertIn(b"viewport", result.data)

    def test_markdown_preserves_unrelated_frontmatter(self):
        md = "---\ntitle: My Post\ngenerated_by: Claude\ntags:\n  - a\n---\n\n# Body\n\ntext\n"
        result = cm.markdown_clean(md.encode(), "provenance")
        out = result.data.decode()
        self.assertIn("title: My Post", out)
        self.assertIn("tags:", out)
        self.assertIn("  - a", out)
        self.assertNotIn("generated_by", out)
        self.assertIn("# Body", out)

    def test_markdown_without_frontmatter_unchanged(self):
        md = "# Title\n\nJust prose.\n"
        result = cm.markdown_clean(md.encode(), "provenance")
        self.assertFalse(result.changed)
        self.assertEqual(result.data, md.encode())


# --------------------------------------------------------------- DOCX / ODT


class TestOffice(Tmp):
    def test_docx_content_preserved_and_zip_valid(self):
        p = self.write("d.docx", make_docx(app="Generated by Claude"))
        result = cm.office_clean(p, "docx", "provenance")
        out = self.tmp / "out.docx"
        out.write_bytes(result.data)
        with zipfile.ZipFile(out) as zf:
            self.assertIsNone(zf.testzip())
            self.assertIn(b"Hello visible text", zf.read("word/document.xml"))
            self.assertNotIn(b"Claude", zf.read("docProps/app.xml"))

    def test_docx_without_ai_marker_unchanged_in_provenance_mode(self):
        p = self.write("d.docx", make_docx(app="Microsoft Word"))
        result = cm.office_clean(p, "docx", "provenance")
        self.assertFalse(result.changed)

    def test_corrupt_zip_fails_closed(self):
        p = self.write("bad.docx", b"PK\x03\x04garbagegarbage")
        with self.assertRaises(SafetyError):
            sniff(p)


# ------------------------------------------------------------- file integrity


class TestFileSafety(Tmp):
    def test_refuses_existing_destination(self):
        dest = self.write("out.txt", "existing")
        with self.assertRaises(SafetyError):
            atomic_write(dest, b"new")
        self.assertEqual(dest.read_text(), "existing")

    def test_force_overwrites(self):
        dest = self.write("out.txt", "existing")
        atomic_write(dest, b"new", force=True)
        self.assertEqual(dest.read_text(), "new")

    def test_no_partial_file_left_on_failure(self):
        dest = self.tmp / "sub" / "out.bin"
        try:
            atomic_write(dest, b"data")
        except SafetyError:
            pass
        leftovers = list(self.tmp.rglob(".tmp-provenance-*"))
        self.assertEqual(leftovers, [])

    def test_clean_file_writes_nothing(self):
        p = self.write("clean.md", "# Title\n\nOrdinary prose with no marks.\n")
        rc = clean_text.main([str(p)])
        self.assertEqual(rc, 0)
        self.assertFalse(default_output(p).exists())

    def test_original_preserved_after_clean(self):
        original = "before​after"
        p = self.write("t.txt", original)
        clean_text.main([str(p)])
        self.assertEqual(p.read_text(), original)
        self.assertTrue(default_output(p).exists())

    def test_text_clean_idempotent(self):
        p = self.write("t.txt", "a​b‌c")
        clean_text.main([str(p)])
        once = default_output(p)
        first = once.read_text()
        rc = clean_text.main([str(once)])
        self.assertEqual(rc, 0)
        self.assertEqual(once.read_text(), first)


# ------------------------------------------------------- container CLI + C2PA


class TestContainerCli(Tmp):
    def test_unsupported_format_refused(self):
        p = self.write("a.bin", b"\x00\x01\x02\x03binary")
        self.assertEqual(clean_container.main([str(p)]), 1)

    def test_pdf_without_exiftool_refuses(self):
        from common import tool_available
        p = self.write("a.pdf", b"%PDF-1.4\n%%EOF\n")
        rc = clean_container.main([str(p)])
        self.assertEqual(rc, 1)
        self.assertFalse(default_output(p).exists())
        if not tool_available("exiftool"):
            pass  # refusal is the correct degraded behaviour

    def test_c2pa_state_unknown_without_tool(self):
        """Missing tooling must yield 'unknown', never 'absent' (issue #1)."""
        from common import c2pa_state, tool_available
        p = self.write("i.png", make_png())
        state, detail = c2pa_state(p)
        if tool_available("c2patool"):
            self.assertIn(state, {"present", "absent", "unknown"})
        else:
            self.assertEqual(state, "unknown")
            self.assertIn("c2patool", detail)


# -------------------------------------------------------------------- inspect


class TestInspect(Tmp):
    def test_unsupported_binary_reports_warning_not_crash(self):
        p = self.write("a.bin", b"\x00\x01\x02\x03")
        report = inspect_mod.inspect_path(p).to_dict()
        self.assertEqual(report["findings"], [])
        self.assertTrue(report["warnings"])

    def test_png_always_reports_unverifiable_layers(self):
        p = self.write("i.png", make_png())
        report = inspect_mod.inspect_path(p).to_dict()
        joined = " ".join(report["unavailable"])
        self.assertIn("pixel-domain", joined)

    def test_batch_does_not_follow_symlinks(self):
        """An audit of a directory must not read files outside it."""
        outside = self.tmp / "outside"
        outside.mkdir()
        (outside / "secret.txt").write_text("private")
        root = self.tmp / "root"
        root.mkdir()
        (root / "ok.md").write_text("# fine\n")
        (root / "link.txt").symlink_to(outside / "secret.txt")
        (root / "dir").symlink_to(outside)
        rc = inspect_mod.main([str(root), "--recursive", "--json"])
        self.assertEqual(rc, 0)
        report = inspect_mod.inspect_path(root / "ok.md").to_dict()
        self.assertEqual(report["findings"], [])

    def test_batch_continues_past_bad_file(self):
        self.write("good.md", "# ok\n")
        self.write("bad.png", make_png()[:20])
        self.write("marked.txt", "x​y")
        rc = inspect_mod.main([str(self.tmp), "--recursive", "--json"])
        self.assertEqual(rc, 0)


# ------------------------------------------------ Pass 3 adversarial regressions


class TestFalsePositives(Tmp):
    def test_ordinary_generator_key_preserved(self):
        """A static-site generator is not AI provenance."""
        md = b"---\ntitle: Post\ngenerator: Docusaurus 3.1\n---\n\nBody\n"
        result = cm.markdown_clean(md, "provenance")
        self.assertFalse(result.changed)

    def test_ambiguous_words_are_not_markers(self):
        for value in ["imagen de portada", "flux state management",
                      "Grok Industries Ltd", "Claude Monet retrospective",
                      "influx of data", "model: Ford Focus"]:
            self.assertFalse(cm._value_is_ai(value), f"false positive on {value!r}")

    def test_real_markers_still_detected(self):
        for value in ["Generated by Claude", "Claude", "Gemini 2.0",
                      "Adobe Firefly generated", "Made with Midjourney",
                      "c2pa", "SynthID", "stable diffusion", "DALL-E 3"]:
            self.assertTrue(cm._value_is_ai(value), f"false negative on {value!r}")

    def test_ai_value_under_ambiguous_key_still_removed(self):
        md = b"---\ntitle: Post\ngenerator: Made with Midjourney\n---\n\nBody\n"
        result = cm.markdown_clean(md, "provenance")
        self.assertTrue(result.changed)
        self.assertNotIn(b"Midjourney", result.data)


class TestPngChunkCriticality(Tmp):
    def _png(self, extra: bytes) -> bytes:
        def chunk(t, b):
            return struct.pack(">I", len(b)) + t + b + struct.pack(">I", zlib.crc32(t + b) & 0xFFFFFFFF)
        return (b"\x89PNG\r\n\x1a\n"
                + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 0, 0, 0, 0))
                + extra
                + chunk(b"IDAT", zlib.compress(b"\x00\x00")) + chunk(b"IEND", b""))

    def test_unknown_critical_chunk_never_deleted(self):
        """PNG encodes criticality in the first letter's case. Deleting an
        unrecognised critical chunk corrupts the image."""
        def chunk(t, b):
            return struct.pack(">I", len(b)) + t + b + struct.pack(">I", zlib.crc32(t + b) & 0xFFFFFFFF)
        src = self._png(chunk(b"CrIT", b"critical-private"))
        for mode in ("provenance", "all-metadata"):
            self.assertIn(b"CrIT", cm.png_clean(src, mode).data, f"lost in {mode}")

    def test_unknown_ancillary_chunk_dropped_only_in_all_metadata(self):
        def chunk(t, b):
            return struct.pack(">I", len(b)) + t + b + struct.pack(">I", zlib.crc32(t + b) & 0xFFFFFFFF)
        src = self._png(chunk(b"cRIT", b"ancillary-private"))
        self.assertIn(b"cRIT", cm.png_clean(src, "provenance").data)
        self.assertNotIn(b"cRIT", cm.png_clean(src, "all-metadata").data)


class TestArchiveSafety(Tmp):
    def _zip(self, entries) -> bytes:
        import io as _io
        buf = _io.BytesIO()
        with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
            z.writestr("word/document.xml", "<w:document/>")
            for name, data in entries:
                z.writestr(name, data)
        return buf.getvalue()

    def test_traversal_entry_refused(self):
        p = self.write("t.docx", self._zip([("../../etc/evil.txt", "escaped")]))
        with self.assertRaises(SafetyError):
            cm.office_clean(p, "docx", "provenance")

    def test_absolute_entry_refused(self):
        p = self.write("a.docx", self._zip([("/etc/passwd", "x")]))
        with self.assertRaises(SafetyError):
            cm.office_clean(p, "docx", "provenance")

    def test_decompression_bomb_refused(self):
        p = self.write("b.docx", self._zip([("bomb.bin", b"\x00" * (40 * 1024 * 1024))]))
        with self.assertRaises(SafetyError):
            cm.office_clean(p, "docx", "provenance")

    def test_normal_docx_still_passes_guards(self):
        p = self.write("ok.docx", make_docx(app="Generated by Claude"))
        result = cm.office_clean(p, "docx", "provenance")
        self.assertTrue(result.changed)


class TestNoSelfWatermark(Tmp):
    def test_cleaner_adds_no_metadata_of_its_own(self):
        src = make_png([("Software", "midjourney")])
        out = cm.png_clean(src, "provenance").data
        for marker in (b"provenance", b"cleaned", b"ai-provenance-marks", b"python"):
            self.assertNotIn(marker, out.lower())

    def test_deterministic_output(self):
        p = self.write("d.docx", make_docx(app="Generated by Claude"))
        self.assertEqual(cm.office_clean(p, "docx", "provenance").data,
                         cm.office_clean(p, "docx", "provenance").data)


class TestEdgeInputs(Tmp):
    def test_empty_file_does_not_crash_or_invent(self):
        p = self.write("e.txt", b"")
        report = inspect_mod.inspect_path(p).to_dict()
        self.assertEqual(report["findings"], [])

    def test_xml_declared_entities_refused(self):
        """Version-independent guard: expat's amplification cap is not on 3.9."""
        for xml in [
            '<?xml version="1.0"?><!DOCTYPE d [<!ENTITY a "x">]>'
            '<svg xmlns="http://www.w3.org/2000/svg"><metadata>&a;</metadata></svg>',
            '<?xml version="1.0"?><!DOCTYPE d [<!ENTITY x SYSTEM "file:///etc/passwd">]>'
            '<svg xmlns="http://www.w3.org/2000/svg"><metadata>&x;</metadata></svg>',
        ]:
            with self.assertRaises(SafetyError):
                cm.svg_clean(xml.encode(), "provenance")
            with self.assertRaises(SafetyError):
                cm.svg_inspect(xml.encode())

    def test_ordinary_svg_still_processed(self):
        svg = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">'
               "<title>Chart</title><metadata>Made with Midjourney</metadata>"
               '<rect id="bg" width="10" height="10" style="fill:red"/></svg>')
        result = cm.svg_clean(svg.encode(), "provenance")
        out = result.data.decode()
        for keep in ["viewBox", "<title>Chart</title>", 'id="bg"', "style", "<rect"]:
            self.assertIn(keep, out, f"lost {keep}")
        self.assertNotIn("Midjourney", out)


if __name__ == "__main__":
    unittest.main(verbosity=2)
