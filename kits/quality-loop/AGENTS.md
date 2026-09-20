# Quality Loop Kit — Agent Profile

Use this profile when the goal is **map first, then ship better code without thrashing**. The headline capability is the **software engineering graph**; clean-code, architecture, review, and loop are how you act on that map.

This Kit wires six skills from this repository (plus skill-doctor). Prefer invoking them by name when the job matches.

## Skill set (required)

| Skill | One-line job |
|-------|----------------|
| `software-engineering-graph` | **Headline.** Map the system / change impact before big edits |
| `clean-code` | Write or refactor for clarity without changing behavior |
| `clean-architecture-code` | Keep dependencies pointing inward |
| `clean-architecture-review` | Review architecture boundaries |
| `code-review` | Evidence-backed review of a diff (findings, not silent rewrites) |
| `loop` | Bounded plan → write → validate → independent review → repair |

Optional companion when shipping PRs: `create-pull-request`, `address-pr-feedback`, `run-change-checks` (not required for this Kit).

## Default quality loop (order)

1. **Graph (default first)** — Run `software-engineering-graph` (or a short graph pass) before coding whenever the change spans modules or behavior is unclear. Skip only for tiny single-file edits.
2. **Change** — Implement with `clean-code` / `clean-architecture-code` as needed. Keep scope tight; use the graph for blast radius.
3. **Validate** — Run the project’s real tests/linters. Don’t claim green without evidence.
4. **Review** — Run `code-review` (and `clean-architecture-review` when boundaries moved). Reviewer must not be the same pass that wrote the code when `loop` is active.
5. **Repair** — If review finds blockers, use `loop` with a small repair budget. Stop when accepted or budget exhausted; don’t thrash.

See `playbooks/graph-review-loop.md` for the step-by-step.

## Hard rules

- Reviews produce findings with evidence — not drive-by rewrites unless the user asked for fixes.
- `loop` owns repair budgets; don’t invent unbounded rewrite cycles.
- Prefer the repo’s nearest `AGENTS.md` over this profile when they conflict on local conventions.
- No fake “official Cursor/Claude” claims. Skills are Jeffrey Ferreiras / Quality Loop Kit materials.

## Install check

After install, run `skill-doctor` (or the Kit’s check script) so the Kit skill folders resolve. If any are missing, stop and fix install before starting a loop.
