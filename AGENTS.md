# Global Agent Instructions

## File Search and Discovery

- Use `rg` (ripgrep) for fast file discovery and text search in terminals — prefer it over `Get-ChildItem`, `find`, or `grep`.
- In agent/IDE context, prefer internal tools such as `grep_search` for in-file text searches and `file_search` for file name patterns.
- Use `semantic_search` for concept-based searches across the workspace.


### Working Principles

- Always use the caveman skill for natural-language replies when that skill is available. Keep technical accuracy intact. Code, commits, and PR text stay normal. The user may override with "stop caveman" or "normal mode"; pause that style for security, irreversible actions, or confusion.
- Do not add license headers unless requested.
- Avoid one-letter identifiers; prefer clear names.
- Keep inline comments to a minimum unless explicitly requested.
- Prefer small, well-scoped commits with clear messages.
- When writing or reviewing code, apply the relevant skills:
  - `write-clean-code` for clean, readable production code
  - `what-would-uncle-bob-do` for SOLID/Clean Code principles
  - `clean-architecture-code` for architecture boundaries

### Planning And Execution

- Always start with a short plan capturing goal, steps, and risks.
- Prefer linter feedback over full builds for fast iteration.

### File And Patch Hygiene

- When adding folders, include a `.gitkeep` for empty directories.

### Tests And Validation

- Run the narrowest tests needed to validate the change.
- If changing unit tests or integration tests, run those tests.
- If changing main code files, run the build.
- Prefer linter feedback over builds when fixing compilation errors. Use builds for final fixes.
- Do not fix unrelated broken tests; document them instead.