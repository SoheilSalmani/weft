"""Classification of invisible and near-invisible characters in text.

The central distinction, and the reason this module is not a blocklist:

  A1  artefact      invisible, carries no meaning here, safe to remove
  A2  load-bearing  invisible, but the text means something different without it

Prior art (guillaumemeyer/watermarks-remover @ 256d90d1, MIT) shipped a single
zero-width sweep and its issue #22 records the result: emoji presentation
selectors and ZWJ were stripped, "visibly altering legitimate emoji". U+200D
joins a woman and a laptop into a single technologist glyph; U+200C is required
to render Persian and many Indic scripts correctly. Removing those is data loss,
not cleaning.

So membership in A1 or A2 is decided per occurrence, from the neighbouring
characters, not from the codepoint alone.
"""

from __future__ import annotations

import unicodedata
from dataclasses import dataclass

ZWSP = "​"
ZWNJ = "‌"
ZWJ = "‍"
WJ = "⁠"
BOM = "﻿"
SHY = "­"
NBSP = " "

# Always artefacts: no script needs them to render correctly.
ALWAYS_ARTEFACT = {
    ZWSP: "zero-width space",
    WJ: "word joiner",
    BOM: "zero-width no-break space / BOM",
    SHY: "soft hyphen",
    "᠎": "Mongolian vowel separator",
    "⁡": "function application",
    "⁢": "invisible times",
    "⁣": "invisible separator",
    "⁤": "invisible plus",
}

# Context dependent: artefact in plain Latin prose, meaningful next to emoji or
# in scripts that use them as joiners.
CONTEXTUAL = {ZWNJ: "zero-width non-joiner", ZWJ: "zero-width joiner"}

# Bidirectional controls. Meaningful in mixed-direction text, an artefact and a
# spoofing vector in text with no RTL content at all.
BIDI = {
    "‪": "left-to-right embedding",
    "‫": "right-to-left embedding",
    "‬": "pop directional formatting",
    "‭": "left-to-right override",
    "‮": "right-to-left override",
    "⁦": "left-to-right isolate",
    "⁧": "right-to-left isolate",
    "⁨": "first strong isolate",
    "⁩": "pop directional isolate",
    "‎": "left-to-right mark",
    "‏": "right-to-left mark",
    "؜": "Arabic letter mark",
}

# Exotic spaces. Visible as whitespace, so removing them changes layout: they
# are reported and normalised only on request, never by default.
SPACES = {
    NBSP: "no-break space",
    " ": "en quad", " ": "em quad", " ": "en space",
    " ": "em space", " ": "three-per-em space", " ": "four-per-em space",
    " ": "six-per-em space", " ": "figure space", " ": "punctuation space",
    " ": "thin space", " ": "hair space", " ": "narrow no-break space",
    " ": "medium mathematical space", "　": "ideographic space",
}

VARIATION_SELECTOR_RANGES = [(0xFE00, 0xFE0F), (0xE0100, 0xE01EF)]
TAG_RANGE = (0xE0000, 0xE007F)

RTL_RANGES = [
    (0x0590, 0x05FF), (0x0600, 0x06FF), (0x0700, 0x074F), (0x0750, 0x077F),
    (0x08A0, 0x08FF), (0xFB1D, 0xFDFF), (0xFE70, 0xFEFF), (0x10800, 0x10FFF),
    (0x1E800, 0x1EFFF),
]

# Scripts where ZWNJ and ZWJ carry orthographic meaning.
JOINER_SCRIPT_RANGES = [
    (0x0600, 0x06FF), (0x0700, 0x074F), (0x0900, 0x097F), (0x0980, 0x09FF),
    (0x0A00, 0x0A7F), (0x0A80, 0x0AFF), (0x0B00, 0x0B7F), (0x0B80, 0x0BFF),
    (0x0C00, 0x0C7F), (0x0C80, 0x0CFF), (0x0D00, 0x0D7F), (0x0D80, 0x0DFF),
    (0x0E00, 0x0E7F), (0x1000, 0x109F), (0x1780, 0x17FF), (0xFB50, 0xFDFF),
    (0xFE70, 0xFEFF),
]

EMOJI_RANGES = [
    (0x1F000, 0x1FAFF), (0x2600, 0x27BF), (0x2B00, 0x2BFF),
    (0x1F1E6, 0x1F1FF), (0xFE0F, 0xFE0F), (0x2190, 0x21FF), (0x2300, 0x23FF),
]


def _in(cp: int, ranges) -> bool:
    return any(lo <= cp <= hi for lo, hi in ranges)


def is_variation_selector(ch: str) -> bool:
    return _in(ord(ch), VARIATION_SELECTOR_RANGES)


def is_tag_char(ch: str) -> bool:
    return TAG_RANGE[0] <= ord(ch) <= TAG_RANGE[1]


def text_has_rtl(text: str) -> bool:
    return any(_in(ord(c), RTL_RANGES) for c in text)


def _neighbour_emoji(text: str, i: int) -> bool:
    """True when the character at i sits inside an emoji sequence."""
    for j in (i - 1, i + 1):
        if 0 <= j < len(text) and _in(ord(text[j]), EMOJI_RANGES):
            return True
    return False


def _neighbour_joiner_script(text: str, i: int) -> bool:
    for j in (i - 1, i + 1):
        if 0 <= j < len(text) and _in(ord(text[j]), JOINER_SCRIPT_RANGES):
            return True
    return False


@dataclass
class Mark:
    index: int
    char: str
    codepoint: str
    name: str
    category: str          # zero_width | bidi | tag | variation_selector | space
    protected: bool        # True => A2, never removed by default
    reason: str = ""


def scan(text: str) -> list[Mark]:
    """Classify every invisible or near-invisible character in the text."""
    has_rtl = text_has_rtl(text)
    marks: list[Mark] = []

    for i, ch in enumerate(text):
        cp = f"U+{ord(ch):04X}"

        if ch in ALWAYS_ARTEFACT:
            marks.append(Mark(i, ch, cp, ALWAYS_ARTEFACT[ch], "zero_width", False))
            continue

        if ch in CONTEXTUAL:
            if _neighbour_emoji(text, i):
                marks.append(Mark(i, ch, cp, CONTEXTUAL[ch], "zero_width", True,
                                  "joins an emoji sequence; removing it splits the glyph"))
            elif _neighbour_joiner_script(text, i):
                marks.append(Mark(i, ch, cp, CONTEXTUAL[ch], "zero_width", True,
                                  "orthographically required by the surrounding script"))
            else:
                marks.append(Mark(i, ch, cp, CONTEXTUAL[ch], "zero_width", False))
            continue

        if ch in BIDI:
            if has_rtl:
                marks.append(Mark(i, ch, cp, BIDI[ch], "bidi", True,
                                  "document contains right-to-left text; this control affects rendering"))
            else:
                marks.append(Mark(i, ch, cp, BIDI[ch], "bidi", False,
                                  "no right-to-left text in this document"))
            continue

        if is_tag_char(ch):
            marks.append(Mark(i, ch, cp, "Unicode tag character", "tag", False,
                              "tag characters have no visible rendering"))
            continue

        if is_variation_selector(ch):
            marks.append(Mark(i, ch, cp, "variation selector", "variation_selector", True,
                              "selects text or emoji presentation of the preceding character"))
            continue

        if ch in SPACES:
            marks.append(Mark(i, ch, cp, SPACES[ch], "space", True,
                              "visible whitespace; normalised only with --normalise-spaces"))

    return marks


def clean(text: str, *, normalise_spaces: bool = False,
          include_protected: bool = False) -> tuple[str, list[Mark]]:
    """Remove artefact marks. Returns (cleaned_text, removed_marks).

    Protected (A2) marks are preserved unless include_protected is set, which
    callers must only pass on an explicit per-file decision by the user.
    """
    marks = scan(text)
    removed: list[Mark] = []
    drop: set[int] = set()
    replace: dict[int, str] = {}

    for m in marks:
        if m.protected and not include_protected:
            if m.category == "space" and normalise_spaces:
                replace[m.index] = " "
                removed.append(m)
            continue
        if m.category == "space":
            if normalise_spaces:
                replace[m.index] = " "
                removed.append(m)
            continue
        drop.add(m.index)
        removed.append(m)

    out = []
    for i, ch in enumerate(text):
        if i in drop:
            continue
        out.append(replace.get(i, ch))
    return "".join(out), removed


def summarise(marks: list[Mark]) -> dict:
    """Aggregate by codepoint for reporting, keeping protection visible."""
    agg: dict[tuple[str, bool], dict] = {}
    for m in marks:
        key = (m.codepoint, m.protected)
        entry = agg.setdefault(key, {
            "codepoint": m.codepoint, "name": m.name, "category": m.category,
            "protected": m.protected, "count": 0, "reason": m.reason,
            "first_index": m.index,
        })
        entry["count"] += 1
    return {"marks": sorted(agg.values(), key=lambda e: e["first_index"])}


def normalisation_note(text: str) -> str | None:
    """Report, without changing anything, whether NFC would alter the text."""
    if unicodedata.normalize("NFC", text) != text:
        return "text is not in NFC form; normalisation is not applied automatically"
    return None
