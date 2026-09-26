---
name: remove-slop
description: Clean AI-generated narration, redundant code, or speculative abstractions from a branch diff. Use when asked to remove AI slop while preserving behavior.
---

# Remove Slop

## Workflow

1. Resolve the requested comparison range or the repository's remote default branch.
2. Inspect only additions and modifications in that diff, plus enough surrounding code to understand local conventions and contracts.
3. Remove or simplify changes only when they are redundant, misleading, speculative, or inconsistent with nearby code.
4. Preserve behavior. Do not delete validation, error handling, logging, or security checks without evidence that the contract already guarantees them or the check is harmful.
5. Run the narrowest tests and lint relevant to the edited files.

Common candidates include narration comments, duplicated guards, pass-through helpers, needless wrappers, excessive headings, and abstractions with no demonstrated variation.

Do not rewrite untouched legacy code, broaden scope into a general refactor, or treat unfamiliar style as proof of AI authorship. Finish with a concise summary and validation result.
