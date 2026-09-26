# A record, written badly and then correctly

The same decision written twice. The first version is not a strawman. It is the shape a draft usually takes on the first attempt, and every defect in it is one of the anti-patterns.

## Before

> ---
> status: Accepted
> ---
>
> # 7. Object storage
>
> ## Context
>
> As part of PLAT-2841 we looked at file uploads. Right now `src/api/uploads/handler.ts:114` writes straight to local disk via `fs.writeFile`, and `src/api/uploads/handler.ts:180` reads it back:
>
> ```ts
> const dest = path.join(UPLOAD_DIR, req.file.name)
> await fs.writeFile(dest, req.file.buffer)
> ```
>
> Priya added this back in the early days because we were a three-person team moving fast and it was the quickest thing that worked. Nobody had time to think about scale.
>
> This is the price of shipping without a plan, and it is now blocking us.
>
> ## Decision
>
> We'll move to object storage. It's a significantly more robust and scalable solution, and it's the industry standard approach.
>
> ## Consequences
>
> - Much better scalability
> - Cleaner code
> - Faster
> - Unblocks horizontal scaling

## After

> ---
> status: Accepted
> date: 2026-03-04
> decision-makers: [Priya Raman, Tom Okafor]
> ---
>
> # 7. Store uploaded files in object storage
>
> ## Context
>
> Uploaded files are written to the local disk of the application server that receives the request. Any later request for a file only succeeds if it reaches the same server.
>
> This makes the application stateful. A second server cannot be added without a shared filesystem, and a server cannot be replaced without copying its files first. Disk capacity is a fixed ceiling on total uploads.
>
> The reason local disk was chosen originally is not recorded.
>
> ## Options considered
>
> **A. Shared network filesystem.** Keeps the current file interface unchanged, so the upload path needs no rewrite. Adds a component that must be kept available, and it becomes a single point of failure for every request that touches a file.
>
> **B. Object storage.** Removes file state from the servers entirely. Requires reworking reads and writes around an object API, and introduces a network call where there was a local disk operation.
>
> ## Decision
>
> The system will store uploaded files in object storage, and application servers will hold no persistent local state.
>
> Option B was chosen over option A because a shared filesystem moves the availability problem rather than removing it. Servers remain unable to be replaced independently, which is the constraint that matters most here.
>
> ## Consequences
>
> Application servers become interchangeable. They can be added or replaced without moving data, and disk capacity stops being a limit on uploads.
>
> Reads and writes now cross the network, so file operations can fail in ways local disk operations could not. Every path that touches a file needs to handle that, and latency for a single small file will be higher than it is today.
>
> Existing files must be migrated before the local disk path is removed, and both paths have to work at once during that period.
>
> The system gains a dependency on an external storage service, including its availability and its cost model.

## What changed, and why

Each correction applies one rule:

| Defect in the first version | Rule applied |
| --- | --- |
| `PLAT-2841` | No tracker identifiers. A reader without access learns nothing from it. |
| File paths, line numbers, code block | Name architectural elements, not files. The paths are stale within a release. |
| "Priya added this because we were moving fast" | State facts, not motives. The reason was not recorded, so the corrected version says so. |
| "a three-person team" | No staffing details. Team size is not an architectural fact. |
| "This is the price of shipping without a plan" | Context is value-neutral. Judgement is not evidence. |
| "significantly more robust and scalable", "industry standard" | Sales Pitch. Removed and replaced with the specific constraint that forced the choice. |
| No alternatives | Options considered. A decision with no visible alternative invites Groundhog Day. |
| Four benefits, no costs | Fairy Tale. The corrected version records the added failure modes, the latency cost, the migration, and the new dependency. |
| Bullet fragments | Full sentences. "Cleaner code" carries no reasoning. |
| "Object storage" as a title | Title states the decision, not the topic. |
| Missing date and decision-makers | Metadata is always filled in. |

The corrected version is longer, and that is the wrong thing to measure. It is longer because it contains reasoning, and shorter in every part that was implementation detail.
