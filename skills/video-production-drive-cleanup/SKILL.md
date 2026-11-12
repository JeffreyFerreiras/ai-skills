---
name: video-production-drive-cleanup
description: Inventory, archive, or clean scoped video production Drive renders and related files from verified evidence. Use when asked to audit Drive usage, archive finals, move originals, or remove linked production assets, not to produce or edit video.
---

# Video production Drive cleanup

Work only inside the named Drive folders and local exports the request scopes. Do not produce, edit, or publish video here.

Jump to the part that matches the request and read that part plus the safety rules that govern it.

- For a read only usage review, read Inventory current files.
- For a quota neutral safety copy or a scoped move, read Verify copies.
- For tracker linked removals, read Update tracker references and Approve cleanup before removal.
- For the closing record, read Report results.

## Scope the work

Base every item on actual source evidence from the request scope. Accept scoped active and archived production chats, manifests, local exports, and named Drive folders. Treat folder listings and tracker rows as claims to check, not as proof.

Protect source folders from renames and broad sweeps. Confirm the library path before any copy, move, or deletion. Stay inside the scoped set. Keep unrelated accounts and libraries out of the task.

## Inventory current files

Run the audit as read only. List current contents and estimate candidate savings. Change nothing in Drive and free no quota.

Distinguish finals from review cuts with file names, folder placement, tracker state, and chat confirmation. Report a render with no located source as absent. Report a render known to exist but not openable as inaccessible. State what was sought and where it was sought. Never invent completion and never substitute a nearby cut for a final.

Record item, status, source identifier or path, destination, bytes, and SHA-256 where useful. Mark likely finals, likely review cuts, and unknowns separately.

## Verify copies

Use supported connectors and existing naming rules. Verify the destination library path before writing. Keep version aware names so a new render never hides an older one.

Reuse an identical file by SHA-256 and bytes without rewriting it. Preserve a differently hashed version as a separate version. Never overwrite a different copy. Verify the final readback with bytes and SHA-256 before recording success.

A copy archive preserves the Drive original and has no Drive quota effect. A move relocates a verified item out of Drive and may place the Drive source in recoverable Trash under the existing Trash behavior. Copy and verify first, confirm the archived copy reads back with matching bytes and SHA-256, then move only the scoped source item.

## Update tracker references

Update tracker paths only when the request authorizes it and the item needs it. Make conditional guidance clear and skip tracker edits for items outside the authorized set.

Update actual archive paths in the authoritative structured tracker. Replace or remove the exact approved item URLs and identifiers in the parsed tracker rows and add only archive paths already verified by readback. Preserve unrelated rows, status values, and formatting. Check live references before removing a file that other records may link to.

Check for concurrent edits and verify the saved content before deleting files linked from that tracker. A tracker edit that did not save blocks the linked deletion for that item.

## Approve cleanup before removal

Permanent deletion needs explicit authorization for the scoped set. Once granted for that set, do not ask for a second permission for the same set. Never treat a copy or a move as permission to permanently delete.

Each deleted item needs exact identity with its current version and contents checked at deletion time. When preservation of the item was requested, each deleted item also needs a verified archive copy. When tracker edits were authorized and relevant, each deleted item also needs verified saved tracker edits.

Scope production asset removal to the specified archived episodes. Preserve active render and QA outputs, reusable style and template assets, and the active tracker and workflow unless the request separately authorizes their removal. Check empty folders recursively before removing them.

Account wide Trash purges and unrelated account scans stay outside a narrow task. When a source changed or a step partly failed, handle that item alone with a fresh readback and an explicit report. Do not retry the whole set as one broad retry.

## Report results

Report the verified result for each item. State what was inventoried, archived, moved, or deleted and give the evidence that confirms it.

Report logical bytes copied or removed separately from provider reported storage or quota. Name what remains unresolved and the next check that would resolve it. Omit personal paths, account identifiers, chat links, and past task counts.
