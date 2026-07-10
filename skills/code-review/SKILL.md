---
name: code-review
description: Review local Git changes, a commit range, or a supplied diff for correctness, regressions, security, performance, maintainability, and project-guideline violations. Use when the user requests a code review, branch review, diff review, pre-merge audit, or evidence-backed findings without implementation.
---

# Code Review

## Workflow

1. Read the applicable `AGENTS.md` and inspect repository status.
2. Use the user-specified range when provided. Otherwise resolve the remote default branch from `refs/remotes/origin/HEAD`, with the current repository's established branch as a fallback.
3. Inspect the diff and enough surrounding code, callers, tests, configuration, and contracts to establish impact.
4. Prioritize behavioral defects and regressions over stylistic preferences.
5. Report only actionable findings supported by a concrete failure mode or maintainability cost.

Do not edit files during a review-only request.

## Review Checks

- Correctness, edge cases, error paths, concurrency, and state transitions.
- Authentication, authorization, input handling, secrets, unsafe execution, and data exposure.
- Compatibility of public APIs, schemas, persistence, and configuration.
- Performance risks on plausible hot paths or unbounded inputs.
- Test coverage for changed behavior and meaningful failure paths.
- Compliance with repository instructions and established local conventions.

## Severity

- Critical: likely compromise, data loss, or broadly broken production behavior.
- High: likely user-visible failure or serious security/correctness regression.
- Medium: conditional defect, compatibility risk, or material maintainability problem.
- Low: localized issue with limited impact. Omit optional style preferences.

## Output

Lead with findings ordered by severity. For each finding, include the file and line, the triggering conditions, impact, and a focused remediation. Then note open questions and residual test risk. If no findings remain, say so explicitly.
