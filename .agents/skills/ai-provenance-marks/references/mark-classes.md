# Mark classes

Four layers, distinguished by whether removal can be verified. The layer decides what you are allowed to claim.

| Layer | What it is | Detect | Remove | Verify |
| --- | --- | --- | --- | --- |
| **A1** artefact Unicode | Zero-width, tag chars, bidi controls in non-RTL text | Exact | Exact | **Yes** |
| **A2** load-bearing Unicode | Emoji joiners, variation selectors, script joiners, bidi in RTL text | Exact | Possible, but it is data loss | n/a |
| **C** container metadata | C2PA hard-bound manifests, XMP, EXIF, generator tags, document properties | Exact | Yes | **Yes, for the container** |
| **B** statistical text | Token-sampling bias in word choice | **No** | Rewrite only | **No** |
| **D** pixel and signal domain | SynthID-class media marks, C2PA soft binding | **No** | Regeneration only | **No** |

A and C are the skill's scope. B and D are named so they can be declined accurately.

## Why A splits in two

A1 and A2 are frequently the same codepoint. U+200D is an artefact between two Latin words and essential between two emoji. U+200C is noise in English and orthography in Persian. A blocklist cannot tell them apart, which is why classification is per occurrence, from the neighbouring characters.

Protected by default:

- **ZWJ / ZWNJ** adjacent to emoji, or to Arabic, Indic, Thai, Khmer or Myanmar characters.
- **Variation selectors** (U+FE00–FE0F, U+E0100–E01EF), which choose text or emoji presentation.
- **Bidi controls** in any document containing right-to-left script.
- **Exotic spaces**, which are visible whitespace; normalised only on request.

Always artefacts: ZWSP, word joiner, BOM inside a document, soft hyphen, invisible operators, and Unicode tag characters (U+E0000–E007F), which have no visible rendering in any script.

## Why C2PA spans two layers

C2PA is one standard with two bindings, and they behave in opposite ways.

- **Hard binding** is a cryptographic hash over the asset, carried in an embedded manifest (JUMBF in APP11 for JPEG, a `caBX` chunk in PNG, dictionaries in PDF). Removing the manifest removes it, verifiably. This is Layer C.
- **Soft binding** is a fingerprint or invisible watermark *in the content*. The specification states it survives re-encoding and scaling, and that a manifest can be recovered by searching a repository keyed on it. This is Layer D, and stripping metadata does not touch it.

The consequence is the sentence the skill must always produce: removing an embedded manifest is verifiable, and removing provenance is not.

## What "no EXIF" does not mean

EXIF, XMP and C2PA are separate carriers in the same file. An asset can hold a C2PA manifest with no EXIF at all. Never infer one from the other, and never infer either from a file's extension.
