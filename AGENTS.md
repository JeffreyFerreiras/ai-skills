# Global Agent Instructions

## File Search and Reading

- Prefer `grep` (or `grep -r` for recursive search) over other tools when searching for text patterns in files.
- Prefer `grep` over built-in read-file tools when scanning file contents for specific strings, symbols, or patterns.
- Use `grep -n` to include line numbers in output.
- Use `grep -l` when only file names are needed.
- Use `grep -r --include="*.ext"` to scope searches to specific file types.

## Imported Guidance From out-2-nite

These instructions were merged from `C:\dev\GitHub\out-2-nite\ai\AGENTS.md`. Keep project-specific paths and memory conventions scoped to that source when they do not apply globally.

### Working Principles

- Be brief and concise. Keep responses focused and to the point. Avoid fluff, filler phrases, unnecessary preamble, and verbose explanations.
- Always use the caveman skill for natural-language replies when that skill is available. Keep technical accuracy intact. Code, commits, and PR text stay normal. The user may override with "stop caveman" or "normal mode"; pause that style for security, irreversible actions, or confusion.
- Use TDD principles to write code.
- Write clean code that is easy to understand, maintain, and test.
- Use purposeful naming and clear code structure.
- Keep changes minimal, focused, and reversible.
- Fix root causes and avoid unrelated refactors.
- Match existing code style and structure.
- Do not add license headers unless requested.
- Avoid one-letter identifiers; prefer clear names.
- Keep inline comments to a minimum unless explicitly requested.
- Prefer small, well-scoped commits with clear messages.
- Investigate before proposing or answering. Read and understand relevant files before proposing code edits or answering questions. If the user references a specific file or path, open and inspect it before explaining or proposing fixes.
- Be rigorous and persistent when searching code for key facts. Review style, conventions, and abstractions before implementing new features or abstractions.
- Avoid over-engineering. Only make changes that are directly requested or clearly necessary.
- Do not add features, refactor code, or make improvements beyond what was asked.
- Do not add error handling, fallbacks, or validation for scenarios that cannot happen. Validate at system boundaries such as user input and external APIs.
- Do not create helpers, utilities, abstractions, or scripts for one-time operations. Reuse existing abstractions where possible and follow DRY.
- Write high-quality, general-purpose solutions that work correctly for all valid inputs, not only test cases.
- Do not hard-code values or create solutions that only work for specific test inputs.
- Focus on understanding requirements. Tests verify correctness; they do not define the solution.
- If a task is unreasonable or infeasible, or if tests are incorrect, say so instead of working around them.

### LLM Usage

- When an LLM is needed, default to Claude Sonnet 4.5 unless the user requests otherwise.
- The model string for Claude Sonnet 4.5 is `claude-sonnet-4-5-20250929`.
- End messages with `claudete` if using Claude Sonnet 4.5. If not using Claude Sonnet 4.5, say `notit`.

### Planning And Execution

- Always start with a short plan capturing goal, steps, and risks.
- Prefer linter feedback over full builds for fast iteration.
- Update the plan as steps complete and when scope changes.
- Before large edits, note intent in the relevant task log when one exists.
- For ambiguous work, propose options and record the chosen path in the relevant decision log when one exists.
- Run terminal commands in parallel when possible. Only run commands sequentially when one depends on the output or completion of another.

### File And Patch Hygiene

- Create or modify only the files required for the task.
- When adding folders, include a `.gitkeep` for empty directories.
- Keep diffs readable and group related changes together.
- Update adjacent docs when behavior or usage changes.
- Clean up temporary files, scripts, and helper files created during iteration.

### Tests And Validation

- Run the narrowest tests needed to validate the change.
- If changing unit tests or integration tests, run those tests.
- If changing main code files, run the build.
- Prefer linter feedback over builds when fixing compilation errors. Use builds for final fixes.
- Do not fix unrelated broken tests; document them instead.
- Verify UI, layout, and frontend component changes in a browser before considering the task done.

### Task Entry Template

```text
## <Short Title>
- ID: <YYYYMMDD-unique>
- Status: todo | in_progress | blocked | done
- Owner: <agent or human>
- Summary: <one-liner>
- Plan:
  1) <step>
  2) <step>
- Notes: <context/links>
- Exit Criteria: <how to validate>
```

### Memory Log Template

```text
### YYYY-MM-DD
- Key context learned:
- Decisions made (and why):
- Follow-ups:
```

### Data And Safety

- Do not store secrets or credentials in the workspace.
- Avoid PII; redact or reference externally when necessary.
- Respect network and filesystem sandboxing; prefer local reasoning.

### Frontend Aesthetics

- Avoid generic AI-generated aesthetics. Make creative, distinctive frontends that feel designed for the context.
- Choose fonts that are beautiful, unique, and interesting. Avoid generic fonts like Arial and Inter when a distinctive choice would improve the result.
- Commit to a cohesive color and theme. Use CSS variables for consistency.
- Use animations for effects and micro-interactions. Prioritize CSS-only solutions for HTML and Motion library for React when available.
- Create atmosphere and depth in backgrounds rather than defaulting to plain solid colors when a richer visual treatment is appropriate.
- Avoid overused font families, cliched color schemes, predictable layouts, and cookie-cutter component patterns.
- Interpret creatively and make unexpected choices that fit the context. Vary between light and dark themes, fonts, and aesthetics.
