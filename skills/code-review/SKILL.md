---
name: code-review
description: Review local Git changes, a commit range, or a supplied diff for correctness, regressions, security, performance, maintainability, design-pattern opportunities, and project-guideline violations. Use when the user requests a code review, branch review, diff review, pre-merge audit, or evidence-backed findings without implementation.
---

# Code Review

## Workflow

1. Read the applicable `AGENTS.md` and inspect repository status.
2. Use the user-specified range when provided. Otherwise resolve the remote default branch from `refs/remotes/origin/HEAD`, with the current repository's established branch as a fallback.
3. Inspect the diff and enough surrounding code, callers, tests, configuration, and contracts to establish impact.
4. Prioritize behavioral defects and regressions over stylistic preferences or pattern suggestions.
5. Report only actionable findings supported by a concrete failure mode or maintainability cost.

Do not edit files during a review-only request.

## Review Checks

- Correctness, edge cases, error paths, concurrency, and state transitions.
- Authentication, authorization, input handling, secrets, unsafe execution, and data exposure.
- Compatibility of public APIs, schemas, persistence, and configuration.
- Performance risks on plausible hot paths or unbounded inputs.
- Test coverage for changed behavior and meaningful failure paths.
- Compliance with repository instructions and established local conventions.
- Concrete design pressure that may justify a known pattern, especially repeated variation, scattered state logic, construction policy, boundary adaptation, event fanout, request lifecycle, or dependency creation.

## Pattern Graph Check

Use the bundled graph only when changed code exposes a material design pressure. Do not search for documentation-only, generated, mechanical, or trivially local changes.

1. Describe the observed pressure in domain terms, including the affected responsibility, expected axis of change, and current cost. Do not start with a desired pattern name.
2. Resolve this skill's directory from the loaded `SKILL.md`, then run:

   ```text
   python <skill-directory>/scripts/search_review_graph.py "<observed pressure>" --depth 1 --max-nodes 8 --json
   ```

   Use the repository's configured Python interpreter when one exists. If Python is unavailable, read `references/review-graph.manifest.json` and apply the same one-hop, typed-edge lookup manually.
3. Treat returned patterns as hypotheses. Confirm the candidate's intent, applicability, tradeoffs, and `avoid_when` conditions against the diff and surrounding code. Similar class shapes or keywords alone are not evidence of fit.
4. Prefer the smallest design that handles demonstrated variation. Do not recommend indirection for hypothetical reuse, stable one-off branches, or complexity that the candidate pattern merely relocates.
5. Search at most three distinct material pressures per review. A repository-specific manifest may be supplied with `--manifest` when the repository documents one.

Report a pattern opportunity only when the current design creates a concrete maintenance or correctness cost, the likely direction of change is visible, and the proposed pattern directly addresses both. Name the pattern, but explain the problem and tradeoff rather than relying on the name as justification. Pattern opportunities never displace higher-severity defects.

## Severity

- Critical: likely compromise, data loss, or broadly broken production behavior.
- High: likely user-visible failure or serious security/correctness regression.
- Medium: conditional defect, compatibility risk, or material maintainability problem.
- Low: localized issue with limited impact. Omit optional style preferences and speculative pattern use.

## Output

Lead with findings ordered by severity. For each finding, include the file and line, the triggering conditions, impact, and a focused remediation. Label an evidence-backed design finding as `Pattern opportunity` and include the matched design pressure, candidate pattern, and material tradeoff. Then note open questions and residual test risk. If no findings remain, say so explicitly.
