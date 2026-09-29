---
name: ai-provenance-marks
description: Inspects and removes AI provenance marks from files the user owns, covering invisible Unicode artefacts and container metadata such as C2PA Content Credentials, XMP packets and AI generator tags in PNG, JPEG, SVG, HTML, Markdown, DOCX and ODT. Reports precisely what was verified and what cannot be. Use when asked to remove an AI watermark, strip AI provenance or Content Credentials, inspect or clean AI metadata, or remove invisible AI markers. Not for photographer logos, artist signatures, stock-photo watermarks or other third-party ownership marks.
license: MIT
compatibility: Requires Python 3.9+. Optional c2patool and exiftool improve C2PA and PDF coverage; without them those states are reported as unknown.
---

# AI provenance marks

Two things are true at once, and the skill exists to hold both. Deterministic marks can be found and removed exactly. Everything else cannot be verified at all. **Never let the first fact lend its confidence to the second.**

## Scope

In scope: **Layer A**, invisible Unicode artefacts in text, and **Layer C**, provenance metadata in containers.

Out of scope, and say so plainly rather than improvising:

- **Statistical text watermarks** (token-sampling, SynthID-Text class). No public detector exists, so presence cannot be established, removal cannot be verified, and rewriting only degrades the user's own text. If asked, explain this. Ordinary rewriting is ordinary editing and does not belong here.
- **Pixel, audio and video watermarks.** Removal means regenerating content.
- **C2PA soft bindings.** They live in the content, survive metadata stripping by design, and can re-link an asset to its manifest.
- **Visible ownership marks**: photographer logos, artist signatures, stock-photo watermarks, copyright notices. Different problem, different rights. Decline and say why.

## Workflow

Inspection alone is a complete answer. Only continue when cleaning was asked for.

```
identify type by magic bytes → inspect → classify → choose the narrowest operation
→ write a new file → validate → re-inspect → report removed, preserved, unverified
```

```bash
SCRIPTS="<skill_dir>/scripts"
python3 "$SCRIPTS/inspect_marks.py" FILE [--json] [--recursive]
python3 "$SCRIPTS/clean_text.py" FILE [-o OUT] [--normalise-spaces] [--include-protected]
python3 "$SCRIPTS/clean_container.py" FILE [-o OUT] [--mode provenance|all-metadata]
```

Always inspect first. The realistic base rate is low: most files carry nothing, and **"no marks detected, nothing to clean" is a successful outcome.** Do not hunt for something to remove.

## What the scripts will not do

They are written to fail closed, so trust their refusals rather than working around them.

- Text tools **refuse binary and container input** by magic bytes, so a PNG renamed `.txt` cannot reach a text transform.
- Cleaning **writes `input.cleaned.ext`** and never overwrites without `--force`. Originals are preserved.
- A clean file produces **no output file at all**.
- PNG `IDAT` and JPEG scan data are **copied byte for byte**. Metadata work never re-encodes pixels.
- Malformed containers raise rather than emitting a repaired guess.
- PDF without `exiftool` is **refused**, not half-stripped.

## Invisible is not the same as meaningless

The default preserves characters that are invisible but load-bearing, and this is the single most important behaviour in the skill. U+200D joins an emoji sequence; U+200C is orthographic in Persian and many Indic scripts; variation selectors choose emoji presentation; bidi controls matter in documents containing right-to-left text. Prior art shipped a blanket zero-width sweep and corrupted legitimate emoji.

Those are reported as `protected` and kept. `--include-protected` exists for a reviewed, per-file decision, never as a default.

Exotic spaces are visible whitespace, so they are reported but only changed with `--normalise-spaces`.

## Metadata modes

`--mode provenance` is the default: C2PA manifests, XMP packets, AI generator tags. **EXIF is preserved**, so orientation, colour and timestamps survive. `--mode all-metadata` removes EXIF too, which is a different request from the user's and must be reported as such. Do not silently widen one into the other.

## Visible text is never metadata

The words `Claude`, `OpenAI` or `Gemini` in prose are prose. A vendor name is a finding only inside a metadata structure, a frontmatter key, or a generator tag. Prior art flagged the visible word "Claude" in body text as AI metadata; do not repeat it.

## Reporting

Report three sections, always, and keep the vocabulary separate: **removed** (what changed), **preserved** (what deliberately did not), **not verified** (what nobody checked).

Use `removed` and `no longer detected` for deterministic results. Use `unknown` when a tool was unavailable. **Never write "watermark successfully removed".** After a C2PA strip the honest sentence is *no embedded manifest remains*, plus the soft-binding caveat.

`unknown` is not `absent`. If `c2patool` is missing, C2PA state is unknown, and the report says so rather than implying a clean file.

Read `references/verification.md` before writing any claim about what was removed. Read `references/mark-classes.md` when classifying an unfamiliar mark, `references/formats.md` for per-format coverage and what each strip touches, and `references/sources.md` for the vendor evidence behind these statements.

## Absence proves nothing

Cleaning changes provenance metadata, not how the content was made. Anthropic's own documentation states that lack of a detected mark does not mean content was not AI-generated. **Never describe a cleaned file as human-authored, undetectable, or verified clean.**

## Dependencies, and the ones to refuse

Only `c2patool` and `exiftool` are optional helpers, and neither is bundled, installed, or
fetched at runtime. Nothing here makes a network call. Check the licence before vendoring
either.

Three dependencies were considered and rejected, and adding any of them would break a
guarantee this skill makes:

- **Pillow and other imaging libraries** re-encode pixels, which defeats the
  minimal-change guarantee. Metadata is edited in the container, never by rewriting the
  image.
- **Pixel-removal backends** of the CtrlRegen kind ship without a licence file, making
  their terms all-rights-reserved, and pull roughly 10 GB of models.
- **Reverse-SynthID scorers** are non-commercial research code and are not official
  detectors. Presenting one as a detector manufactures confidence the skill exists to
  refuse.

Run the suite with `python3 tests/test_provenance.py` from the skill directory. Standard
library only, no install step, and every fixture is generated in the test file so no
third-party material is committed. It runs identically with and without the optional
tools, and asserts the degraded behaviour when they are absent, which is the default on a
clean machine.

## Responsible use

Intended for content the user owns or is authorised to process. Metadata hygiene and privacy are the purpose. If a request is clearly about passing work off as human-written, or about removing someone else's ownership mark, say the skill does not cover it and stop. One sentence, then move on.

## Before you finish

- Inspection ran before any mutation, and the report distinguished confirmed from probable.
- The narrowest applicable operation was used; no pixels re-encoded, no prose rewritten.
- Protected characters were preserved unless the user decided otherwise per file.
- The original file still exists and the output went to a new path.
- Output was re-inspected, and every claim maps to something actually checked.
- Unverifiable layers were named, not omitted.
