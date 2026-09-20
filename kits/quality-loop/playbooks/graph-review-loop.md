# Playbook: Graph → Review → Loop

Goal: change code once, review once, repair a few times max — then stop.

## When to use
- Multi-file or behavior-changing work
- “Make it clean / merge-ready” requests
- Anything that previously caused rewrite thrash

## Steps
1. **Freeze the ask** — acceptance checks, in-scope paths, out-of-scope paths.
2. **Graph (as needed)** — `software-engineering-graph` for impact and seams. Skip only for tiny single-file edits.
3. **Write** — smallest change that meets acceptance. Use `clean-code` / `clean-architecture-code` while writing.
4. **Validate** — project tests / typecheck / lint. Capture commands + exit codes.
5. **Review** — fresh `code-review` pass on the diff. Add `clean-architecture-review` if module boundaries moved.
6. **Decide**
   - No blockers → done (optionally open PR).
   - Blockers → enter `loop` with repair budget (default max 3). Same acceptance criteria; no scope creep.
7. **Stop** — accepted, or budget used. Report leftover risks; don’t keep rewriting for style.

## Anti-patterns
- Reviewer and writer as the same unchecked pass inside `loop`
- Expanding scope mid-repair
- Claiming tests passed without running them
