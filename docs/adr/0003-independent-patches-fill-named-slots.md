---
status: Proposed
date: 2026-09-29
decision-makers: [Soheil Salmani]
---

# ADR-0003: Let independent patches add lines to one file through named slots

## Context

A patch is a list of operations on the rendered tree, and file content is a list of lines made of literal text, answer references and expressions. Only one patch can create a given file. Every other patch changes it with hunks anchored on the lines around the change. Two patches with no dependency path between them must commute, and validation proves it by applying each such pair in both orders and comparing the trees.

Two independent patches that insert after the same anchor line do not commute, because the order they apply in decides which line comes first. Validation rejects them. Templates in use on 2026-09-29 worked around this in three ways. One owning patch wrote every entry of a shared configuration file as an expression over several answers. An entry whose owner could be gated off was split into two patches with complementary gates, one that creates the file and one that edits it. Patches that each add a line to one list, such as the tools a toolchain file pins or the dependencies of a build script, were chained by dependencies whose only purpose was to order those lines.

A hunk recorded next to lines an expression rendered anchors on text that exists only under the answers it was recorded with, so the same answer-dependence reaches the patches that edit such files.

The engine knows lines and segments, not file formats. The files involved include JSON, TOML, Kotlin build scripts, ignore files and Markdown. Patch identities are content hashes that scaffolded projects pin, so a change to the patch format must leave existing identities unchanged.

## Options considered

### A. Named slots at the line level

The owning patch declares a named slot as a line of the content it adds. Other patches add a contribution to the slot under a key. The slot renders its contributions sorted by key, with an optional separator between them, and the owner can mark its file to be left out while its slots are empty. No file is parsed.

### B. A structured entry operation

An operation sets a key in a parsed data file, creating the file and any missing parents, and writes the file back with keys in a fixed order. It merges nested values and detects two patches setting one key. It needs parsing and serialising code per format, and a comment-preserving editor for TOML. The serialiser decides the file's layout. Recording would have to recognise structured files, and the operation does nothing for files that are not data, such as build scripts.

### C. A create-or-edit operation

An operation creates a file when it is missing and edits it otherwise. This removes the need for complementary gated variants, but the lines two independent patches insert still land in an order that depends on which patch applies first.

## Decision

The system will let a patch declare named slots and let other patches fill them (option A).

A slot is declared as a line of its own in the content a patch creates or in the lines a hunk adds. A fill names the file, the slot, a key and its lines. Contributions render in key order, the separator ends every contribution but the last, and an empty slot renders no lines. A file whose listed slots are all empty is left out. No operation sees what a slot holds, so hunks cannot anchor on contributions, and patches that only fill slots commute by construction. Validation counts those pairs instead of rendering them. Two contributions under one key in one slot are an error that names both patches.

Recording treats every line added between a slot's neighbours as a contribution to it, keyed by the name of the patch being recorded. The lines must sit where that key sorts. Recording never takes hunk context from slot content.

Slots may be declared in the lines a hunk adds, so a patch that edits a file can offer a place in it. Slots do not nest. A file left out because its slots are empty while another patch changed it by other means is a render error, rather than a silent loss of that change. Keys may interpolate answers, so an integration patch rendered once per instance can contribute once per instance.

Option B was not chosen because it needs code for every format, takes the layout of each file away from its author, and does nothing for files that are not data. It remains available if key-level overrides or nested merging become necessary. Option C was not chosen because it leaves the order of contributions to the order patches apply in.

## Consequences

Independent contributions to one file become sibling patches that each depend only on the owner. The owner can be ungated, and dependency chains that existed only to order lines can be removed.

A worktree holds text, so recording alone cannot reproduce a slot declaration. How the declaration gets written without the author editing a patch file, and how an owner is amended, is decided separately in ADR-0005. Regenerating a patch from a command's output cannot carry a slot either, so a generated patch that declares one is left alone by regeneration.

Every line added between a slot's neighbours belongs to the slot. An owner that wants room for other additions next to a slot has to put a literal line between the two.

Contributions appear in key order, which recording takes from the patch name. An author cannot choose a different position, and one patch cannot override another patch's entry.

The engine does not check that a rendered file is valid JSON, TOML or anything else. A separator placed wrongly by hand, or content that breaks the file's syntax, is only found by whatever reads the file.

A release of the tool from before this decision cannot load a patch that declares or fills a slot. Existing patch identities are unchanged, because the new fields are left out of a patch's canonical form when empty.

Rendering now keeps each file's slots open until the tree is finished. Programs that embed the engine and applied operations to a tree directly have to apply them to an unfinished render and finish it.
