#!/usr/bin/env python3
"""Read-only helper for the anki-flashcards skill.

Subcommands:
  check                     Is AnkiConnect reachable, and which profile is open.
  inspect <deck>            What note types a deck uses, with real field names.
  topics [--like PATTERN]   Existing Topic and Tool values, so you reuse instead of coining.
  lint <draft.json>         Check drafted notes against the mechanical house rules.

Never writes to the collection. Adding notes stays with the Anki MCP tools so that the
approval gate in SKILL.md cannot be bypassed by a script.

Exit codes for lint: 0 clean, 1 errors found, 2 bad usage or unreadable draft.
With --strict, warnings also exit 1.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import unicodedata
import urllib.error
import urllib.request
from collections import Counter, defaultdict

ANKI_URL = "http://localhost:8765"

LEGACY_MODELS = {
    "Basic": "no context field, so authors end up prefixing the Front with the topic by hand",
    "Cloze": "superseded by 'Cloze by topic'",
    "Basic (and reversed card)": "use one cloze note with c1 and c2 instead",
    "Basic (optional reversed card)": "unused in this collection",
    "Basic (type in the answer)": "unused in this collection",
    "Basic (and reversed card) (without question in answer)": "unused in this collection",
}

# Field that carries the rendered context line, per note type suffix.
CONTEXT_FIELDS = ("Topic", "Tool", "License", "Body part")

CLOZE_MODELS_WITHOUT_BACK_EXTRA = {
    "Cloze by topic",
    "Cloze (for a tool)",
    "Cloze (for a license)",
    "Cloze (for a body part)",
}

EMOJI_RE = re.compile(
    "["
    "\U0001f300-\U0001faff"
    "\U0001f000-\U0001f2ff"
    "☀-➿"
    "⬀-⯿"
    "️⃣"
    "]"
)
CLOZE_RE = re.compile(r"\{\{c(\d+)::")
TAG_RE = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")
BINARY_RE = re.compile(r"^\s*(true or false|vrai ou faux|yes or no)\b", re.I)
BOLD_LI_RE = re.compile(r"<li>\s*<(b|strong)>[^<]*:", re.I)
HEADING_RE = re.compile(r"</?h[1-6]\b", re.I)
CONJUNCTION_RE = re.compile(r"\b(and|or|et|ou)\b|/", re.I)
# Phrasings where a conjunction is one question, not two.
CONJUNCTION_OK_RE = re.compile(
    r"\b(difference|differences|relationship|distinction|between|versus|vs\.?|compare|"
    r"trade-?off|entre|diff[ée]rence)\b",
    re.I,
)
FILLER_RE = re.compile(
    r"\b(note that|it is important to note|keep in mind that|basically|essentially)\b", re.I
)
# "Sanofi R&D: what does ..." and friends. The label belongs in the context field.
TOPIC_PREFIX_RE = re.compile(r"^\s*([A-Z][^?.!:]{2,40}:)\s")
SET_SEPARATOR_RE = re.compile(r"[·•;]|,\s|<br\s*/?>", re.I)

BACK_WARN, BACK_ERROR = 200, 400
FRONT_WARN = 160


def call(action: str, **params):
    payload = json.dumps({"action": action, "version": 6, "params": params}).encode()
    req = urllib.request.Request(ANKI_URL, data=payload, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=10) as fh:
        body = json.load(fh)
    if body.get("error"):
        raise RuntimeError(body["error"])
    return body["result"]


def connected() -> bool:
    try:
        call("version")
        return True
    except Exception:
        return False


def strip_html(text: str) -> str:
    text = re.sub(r"<[^>]+>", " ", text)
    text = text.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
    return re.sub(r"\s+", " ", text).strip()


def normalise(text: str) -> str:
    text = strip_html(text).lower()
    text = "".join(c for c in unicodedata.normalize("NFD", text) if not unicodedata.combining(c))
    return re.sub(r"[^a-z0-9 ]", "", text)


# ---------------------------------------------------------------- check / inspect


def cmd_check(_args) -> int:
    if not connected():
        print("AnkiConnect is not reachable at " + ANKI_URL)
        print("Anki is probably closed. Launch it with:  open -a Anki")
        print("Then wait for port 8765 to answer before retrying.")
        return 1
    print("AnkiConnect: up (API version %s)" % call("version"))
    print("Profile:     %s" % call("getActiveProfile"))
    print("Decks:       %d" % len(call("deckNames")))
    print("Notes:       %d" % len(call("findNotes", query="deck:*")))
    return 0


def cmd_inspect(args) -> int:
    if not connected():
        print("AnkiConnect is not reachable. Run 'check' first.")
        return 1
    deck = args.deck
    decks = call("deckNames")
    matches = [d for d in decks if d == deck or d.startswith(deck + "::")]
    if not matches:
        near = [d for d in decks if deck.lower() in d.lower()]
        print("No deck named %r." % deck)
        if near:
            print("Did you mean one of:")
            for d in near[:15]:
                print("  " + d)
        else:
            print("This deck does not exist yet, so it will be created on first add.")
        return 1

    query = '"deck:%s"' % deck
    ids = call("findNotes", query=query)
    print("Deck:  %s  (%d notes, %d subdecks)" % (deck, len(ids), len(matches) - 1))
    if not ids:
        print("Empty deck. Pick the note type from references/note-types.md.")
        return 0

    by_model: Counter = Counter()
    context_values: defaultdict = defaultdict(Counter)
    for start in range(0, len(ids), 100):
        for note in call("notesInfo", notes=ids[start : start + 100]):
            by_model[note["modelName"]] += 1
            for field in CONTEXT_FIELDS:
                if field in note["fields"]:
                    value = strip_html(note["fields"][field]["value"])
                    if value:
                        context_values[field][value] += 1

    print("\nNote types in use:")
    for model, count in by_model.most_common():
        flag = "  <-- LEGACY, do not use for new cards" if model in LEGACY_MODELS else ""
        print("  %4d  %s%s" % (count, model, flag))
        try:
            print("        fields: %s" % ", ".join(call("modelFieldNames", modelName=model)))
        except RuntimeError:
            pass

    for field, values in context_values.items():
        print("\nExisting %s values in this deck (reuse one of these):" % field)
        for value, count in values.most_common(15):
            print("  %4d  %s" % (count, value))
    return 0


def cmd_topics(args) -> int:
    if not connected():
        print("AnkiConnect is not reachable. Run 'check' first.")
        return 1
    pattern = (args.like or "").lower()
    found: defaultdict = defaultdict(Counter)
    for model in call("modelNames"):
        fields = call("modelFieldNames", modelName=model)
        context = next((f for f in CONTEXT_FIELDS if f in fields), None)
        if not context:
            continue
        ids = call("findNotes", query='note:"%s" -deck:_Legacy -deck:_Legacy::*' % model)
        for start in range(0, len(ids), 100):
            for note in call("notesInfo", notes=ids[start : start + 100]):
                value = strip_html(note["fields"].get(context, {}).get("value", ""))
                if value and (not pattern or pattern in value.lower()):
                    found[context][value] += 1
    if not found:
        print("No matching %s values." % " or ".join(CONTEXT_FIELDS))
        return 0
    for field, values in sorted(found.items()):
        print("%s values%s:" % (field, " matching %r" % args.like if args.like else ""))
        for value, count in values.most_common(40):
            print("  %4d  %s" % (count, value))
        print()
    return 0


# ------------------------------------------------------------------------- lint


class Report:
    """Collects findings, grouping repeats so one systemic slip is one line."""

    def __init__(self) -> None:
        self.errors: defaultdict = defaultdict(list)
        self.warnings: defaultdict = defaultdict(list)

    def error(self, where: str, message: str) -> None:
        self.errors[message].append(where)

    def warn(self, where: str, message: str) -> None:
        self.warnings[message].append(where)

    @staticmethod
    def render(grouped: defaultdict, prefix: str) -> tuple[list[str], int]:
        lines, total = [], 0
        for message, wheres in grouped.items():
            total += len(wheres)
            if len(wheres) > 3:
                lines.append("%s%s  [%d notes: %s and %d more]" % (
                    prefix, message, len(wheres), ", ".join(wheres[:3]), len(wheres) - 3))
            else:
                lines.append("%s%s  [%s]" % (prefix, message, ", ".join(wheres)))
        return lines, total


def load_batches(path: str) -> list[dict]:
    with open(path, encoding="utf-8") as fh:
        data = json.load(fh)
    return data if isinstance(data, list) else [data]


def lint_text(where: str, label: str, raw: str, report: Report) -> None:
    if re.search(r"<(b|strong)\b", raw, re.I):
        report.error(where, "%s uses bold. Bold is not used in this collection." % label)
    if BOLD_LI_RE.search(raw):
        report.error(where, "%s uses a bold-header bullet list." % label)
    if HEADING_RE.search(raw):
        report.error(where, "%s contains a heading tag. Fields are not documents." % label)
    if EMOJI_RE.search(raw):
        report.error(where, "%s contains an emoji." % label)
    if "—" in raw or "–" in raw:
        report.error(where, "%s contains an em or en dash. Use a comma, period, or parentheses." % label)
    for ch, name in (("‘", "curly"), ("’", "curly"), ("“", "curly"), ("”", "curly")):
        if ch in raw:
            report.error(where, "%s contains a %s quote. Use straight quotes." % (label, name))
            break
    if FILLER_RE.search(strip_html(raw)):
        report.warn(where, "%s contains filler (\"note that\", \"basically\", and similar)." % label)
    if re.search(r"<br\s*/?>\s*<br\s*/?>", raw, re.I):
        report.warn(where, "%s has a blank line in it, which usually means two facts." % label)


def lint_note(where: str, note: dict, model: str, model_fields: list[str] | None, report: Report) -> None:
    fields = note.get("fields") or {}
    if not isinstance(fields, dict) or not fields:
        report.error(where, "no fields.")
        return

    if model_fields is not None:
        unknown = [k for k in fields if k not in model_fields]
        for key in unknown:
            hint = ""
            if key == "Back Extra" and model in CLOZE_MODELS_WITHOUT_BACK_EXTRA:
                hint = " This note type has no Back Extra field, and sending it rejects the whole batch."
            report.error(where, "field %r does not exist on %r.%s" % (key, model, hint))
        missing = [f for f in model_fields if f not in fields]
        if missing and model_fields:
            first = model_fields[0]
            if first in missing:
                report.error(where, "required field %r is missing." % first)

    for key, raw in fields.items():
        if not isinstance(raw, str):
            report.error(where, "field %r is not a string." % key)
            continue
        if not raw.strip() and key in (model_fields or [])[:1]:
            report.error(where, "field %r is empty." % key)
        lint_text(where, "field %r" % key, raw, report)

    context_field = next((f for f in CONTEXT_FIELDS if f in fields), None)
    context = strip_html(fields.get(context_field, "")) if context_field else ""

    front_key = "Front" if "Front" in fields else ("Text" if "Text" in fields else None)
    front_raw = fields.get(front_key, "") if front_key else ""
    front = strip_html(front_raw)

    # The failure mode is the context used as a label prefix, not the context mentioned
    # naturally mid-sentence. "What does --frozen do in uv run?" is good writing;
    # "uv: what does --frozen do?" duplicates the rendered context line.
    if context and front and normalise(context):
        lead = re.match(r"^\s*%s\s*[:\-–,]\s*" % re.escape(context), front, re.I)
        if lead:
            report.error(
                where,
                "%s starts with the %s value as a label (%r). The note type already renders it "
                "above the question." % (front_key, context_field, context),
            )
        elif normalise(front) == normalise(context):
            report.error(where, "%s is just the %s value." % (front_key, context_field))
    if front and front_key == "Front":
        # Catches the topic prefix even on note types with no context field, which is
        # exactly where the habit comes from.
        prefix = TOPIC_PREFIX_RE.match(front_raw)
        if prefix and "<code" not in prefix.group(0):
            report.warn(
                where,
                "Front starts with %r, which looks like a topic label. Move it into the "
                "%s field and use a note type that has one."
                % (prefix.group(1).strip(), context_field or "Topic"),
            )
    # These read as questions, so they apply to a Front and not to a cloze sentence.
    if front and front_key == "Front":
        if BINARY_RE.match(front):
            report.error(where, "Front is a true-or-false card. Rephrase as an open question.")
        if re.match(r"^\s*(is|are|does|do|did|can|will|was|were|should)\b", front, re.I) and front.count("?") == 1:
            report.warn(where, "Front looks like a yes-or-no question. Rephrase as open.")
        if front.count("?") > 1:
            report.error(where, "Front contains more than one question mark. Split it.")
        # Ignore slashes inside numbers and code, where they are not conjunctions.
        prose = re.sub(r"<code>.*?</code>", " ", front_raw, flags=re.S)
        prose = re.sub(r"\d\s*/\s*\d", " ", strip_html(prose))
        if CONJUNCTION_RE.search(prose) and not CONJUNCTION_OK_RE.search(prose):
            report.warn(
                where,
                "Front contains a conjunction or slash, which often means two cards. "
                "Confirm it is one retrieval.",
            )
        if len(front) > FRONT_WARN:
            report.warn(where, "Front is %d characters. Tighten it." % len(front))

    if "Back" in fields:
        back = strip_html(fields["Back"])
        if len(back) > BACK_ERROR:
            report.error(where, "Back is %d characters. That is a bundle, not a card." % len(back))
        elif len(back) > BACK_WARN:
            report.warn(where, "Back is %d characters. Check it is one fact." % len(back))
        if back and front and normalise(back) and normalise(back) in normalise(front):
            report.error(where, "the answer appears inside the Front.")
        raw_back = fields["Back"]
        if "<ul" not in raw_back.lower() and len(SET_SEPARATOR_RE.findall(raw_back)) >= 3:
            report.warn(
                where,
                "Back lists four or more items, which is a set. Asking for a whole set at once "
                "does not survive review. Split into one cloze card per member, or cut it.",
            )

    if "Text" in fields:
        text = fields["Text"]
        indexes = sorted({int(n) for n in CLOZE_RE.findall(text)})
        if not indexes:
            report.error(where, "cloze note has no {{cN::...}} deletion in Text.")
        else:
            if text.count("{{") != text.count("}}"):
                report.error(where, "unbalanced cloze braces in Text.")
            if indexes[0] != 1:
                report.error(where, "cloze indexes start at c%d, not c1." % indexes[0])
            gaps = [i for i in range(1, indexes[-1] + 1) if i not in indexes]
            if gaps:
                report.error(where, "cloze indexes skip %s." % ", ".join("c%d" % g for g in gaps))
            if len(indexes) > 3:
                report.warn(where, "%d cloze indexes on one note. Consider splitting." % len(indexes))
            if strip_html(re.sub(r"\{\{c\d+::|\}\}", "", text)).strip() == strip_html(text).strip():
                pass

    tags = note.get("tags") or []
    if not isinstance(tags, list):
        report.error(where, "tags must be a list.")
        tags = []
    for tag in tags:
        if not TAG_RE.match(str(tag)):
            report.error(
                where, "tag %r is not lowercase kebab-case. Use for example %r."
                % (tag, re.sub(r"[^a-z0-9]+", "-", str(tag).lower()).strip("-") or "source-slug")
            )
    if not tags:
        report.warn(where, "no tags. Every card needs a source tag, for example 'confluence-rcgrwe'.")


def cmd_lint(args) -> int:
    try:
        batches = load_batches(args.draft)
    except (OSError, json.JSONDecodeError) as exc:
        print("Cannot read %s: %s" % (args.draft, exc))
        return 2

    report = Report()
    have_anki = connected()
    if not have_anki:
        print("Note: Anki is closed, so field names and existing duplicates were not verified.\n")

    seen_fronts: dict[str, str] = {}
    total = 0

    for batch_no, batch in enumerate(batches, 1):
        model = batch.get("modelName")
        deck = batch.get("deckName")
        notes = batch.get("notes") or []
        label = "batch %d" % batch_no
        if not model:
            report.error(label, "no modelName.")
        if not deck:
            report.error(label, "no deckName.")
        if model in LEGACY_MODELS:
            report.error(label, "model %r is legacy: %s." % (model, LEGACY_MODELS[model]))
        if len(notes) > 100:
            report.error(label, "%d notes. addNotes takes at most 100 per call." % len(notes))

        model_fields = None
        if have_anki and model:
            try:
                model_fields = call("modelFieldNames", modelName=model)
            except RuntimeError:
                report.error(label, "model %r does not exist in the collection." % model)

        for i, note in enumerate(notes, 1):
            total += 1
            where = "%s note %d" % (label, i)
            lint_note(where, note, model or "", model_fields, report)

            fields = note.get("fields") or {}
            front_raw = fields.get("Front") or fields.get("Text") or ""
            key = normalise(front_raw)
            if key:
                if key in seen_fronts:
                    report.error(where, "duplicate Front, same as %s." % seen_fronts[key])
                else:
                    for other_key, other_where in seen_fronts.items():
                        if similar(key, other_key):
                            report.warn(
                                where,
                                "Front is very close to %s. Sibling cards this similar interfere."
                                % other_where,
                            )
                            break
                    seen_fronts[key] = where

    print("Linted %d note(s) across %d batch(es)." % (total, len(batches)))
    error_lines, error_count = Report.render(report.errors, "  ERROR  ")
    warn_lines, warn_count = Report.render(report.warnings, "  WARN   ")
    if error_lines:
        print("\n%d error(s) in %d distinct kind(s):" % (error_count, len(error_lines)))
        for line in error_lines:
            print(line)
    if warn_lines:
        print("\n%d warning(s) in %d distinct kind(s):" % (warn_count, len(warn_lines)))
        for line in warn_lines:
            print(line)
    if not report.errors and not report.warnings:
        print("Clean. Mechanical checks only, so still run the self-audit in SKILL.md.")
    elif not report.errors:
        print("\nNo errors. Review each warning and decide, do not ignore them by default.")

    if report.errors:
        return 1
    if args.strict and report.warnings:
        return 1
    return 0


def similar(a: str, b: str) -> bool:
    """Token overlap high enough that two cards would interfere."""
    ta, tb = set(a.split()), set(b.split())
    if len(ta) < 4 or len(tb) < 4:
        return False
    overlap = len(ta & tb) / max(len(ta), len(tb))
    return overlap >= 0.8


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("check", help="is AnkiConnect up").set_defaults(func=cmd_check)

    p_inspect = sub.add_parser("inspect", help="note types and context values used by a deck")
    p_inspect.add_argument("deck")
    p_inspect.set_defaults(func=cmd_inspect)

    p_topics = sub.add_parser("topics", help="existing Topic and Tool values")
    p_topics.add_argument("--like", help="substring filter")
    p_topics.set_defaults(func=cmd_topics)

    p_lint = sub.add_parser("lint", help="check a draft JSON file")
    p_lint.add_argument("draft")
    p_lint.add_argument("--strict", action="store_true", help="exit non-zero on warnings too")
    p_lint.set_defaults(func=cmd_lint)

    args = parser.parse_args()
    try:
        return args.func(args)
    except urllib.error.URLError:
        print("Cannot reach AnkiConnect at %s. Is Anki running?" % ANKI_URL)
        return 1
    except RuntimeError as exc:
        print("AnkiConnect error: %s" % exc)
        return 1


if __name__ == "__main__":
    sys.exit(main())
