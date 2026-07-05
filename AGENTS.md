# Global Agent Instructions


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
- Verify UI, layout, and frontend component changes in a browser before considering the task done.

### Frontend Aesthetics

- Avoid generic AI-generated aesthetics. Make creative, distinctive frontends that feel designed for the context.
- Choose fonts that are beautiful, unique, and interesting. Avoid generic fonts like Arial and Inter when a distinctive choice would improve the result.
- Commit to a cohesive color and theme. Use CSS variables for consistency.
- Use animations for effects and micro-interactions. Prioritize CSS-only solutions for HTML and Motion library for React when available.
- Create atmosphere and depth in backgrounds rather than defaulting to plain solid colors when a richer visual treatment is appropriate.
- Avoid overused font families, cliched color schemes, predictable layouts, and cookie-cutter component patterns.
- Interpret creatively and make unexpected choices that fit the context. Vary between light and dark themes, fonts, and aesthetics.
