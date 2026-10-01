---
status: Proposed
date: 2026-09-30
decision-makers: [Soheil Salmani]
---

# ADR-0005: Derive a shared file's slot from the files its patches write

## Context

ADR-0003 lets independent patches add lines to one file: an owning patch declares a named slot as a line of the content it adds, and the other patches fill it. On 2026-09-30 the slot declaration existed only in the owner's patch file. Recording derives a patch from the files in a worktree, which hold text and no slot, so an author wrote the declaration into the patch file by hand, and amending or regenerating an owner was refused because re-recording it would drop the slot.

Authors record patches by editing real files and do not otherwise touch patch files. The one hand edit that slots required was the step authors wanted gone.

Two independent patches that create the same file clash whenever both are active. Recording accepted the second one without comment, because the first was inactive in its session, and validation reported the clash only under answers that turned both on.

The files involved are JSON, TOML, build scripts, ignore files and Markdown, and the engine does not parse any of them. In JSON, entries of a list or map need a separator that neither patch's version of the file contains, since each version has a single entry.

## Options considered

### A. Derive the owner from the patches' versions and a combined file

When two or more independent patches create one file, the lines their versions share before and after their own become the owner's content around a slot, and each patch's own lines become its contribution. Where those lines end, and the separator between contributions, are taken from a combined file: the file as a project with every one of the patches active reads it. The tool proposes one; the author confirms or corrects it.

### B. A patch that exists only while one of its dependents is active

A new kind of patch, written by the tool, that applies only when some patch depending on it applies. Today a patch applies when its gate holds and its dependencies apply, so activity flows from dependencies to dependents. This option makes it flow the other way as well, and the two rules then depend on each other. For files, leaving a file out while its slots are empty, which ADR-0003 already provides, gives the same result.

### C. A marker line typed into the worktree

The author types a line of a reserved form into the file while recording, and recording turns it into a slot. This is exact and cheap to implement, but it is syntax the author has to learn and type into a real file.

### D. Merge data files by their structure

Option B of ADR-0003. It needs a parser and a writer for each format, the writer decides each file's layout, and it does nothing for files that are not data.

## Decision

The system will derive a shared file's slot from the versions its patches create and from a combined file the author confirms (option A).

A command shares one or more files, and the commit that records a second patch creating a file can share it too, when asked or when a person at a terminal agrees. The tool writes a new owning patch, named by the author, whose content is the shared lines around one slot and whose file is left out while no contribution is active. Each patch that created the file instead contributes its own lines under its name and depends on the owner. Each of them alone therefore renders exactly what it rendered before, and the combination that used to clash now renders every contribution.

The tool compares the versions line by line for the lines they share at the start and at the end. It picks where the shared lines end, and the separator, by reproducing the combined file exactly. When several readings reproduce it, it prefers contributions that close every bracket they open, then a separator on one line, then the most shared lines. Its first proposal applies the same preferences and uses a comma between contributions in JSON files. A person reviews the combined file in their editor; an unattended run takes the proposal or a combined file it is given.

Amending an owner shows every contribution in its slots, or a marker line where a slot has none, and records the owner with the slot where those lines are. Changing them is refused, since they belong to other patches.

Option B was not chosen because leaving an empty file out already gives files the behaviour it offers, without a second rule about when patches apply. Option C was not chosen because it asks the author to learn syntax. Option D was not chosen for the reasons ADR-0003 gives.

## Consequences

No workflow requires writing a slot declaration by hand. The patch format still has one, and a hand-written slot keeps working.

Sharing rewrites the patches that created the file, so their identities change. A project renders them to the same files, and its next update writes nothing for them.

A patch inherited from a base template, or one that belongs to an included template, cannot be rewritten from the template that uses it, so a file one of them creates is shared in that template instead. A patch generated by a command must be detached from the command first, because regenerating it would write the whole file again.

The first proposal can be wrong where brackets do not separate entries from the shared lines, or where the separator is not the default for the file type. A person corrects it in the combined file; an unattended run that takes a wrong proposal writes a file that the program reading it may reject.

A shared file has one slot. Versions that differ in two separate places put everything between those places into each contribution, including lines the versions share, and the author sees this in the combined file before anything is written.

Only files that several patches create are detected. Two patches inserting lines at the same place in an existing file are still reported by validation alone.

Committing without a terminal and without asking to share only notes the clash, so the template fails validation until the file is shared or the patches are made alternatives.

An amend of an owner rejects edits to the contributions it shows, which are changed by amending the patches they belong to.
