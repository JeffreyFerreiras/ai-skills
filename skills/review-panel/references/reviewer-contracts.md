# Reviewer contracts

Seven independent role contracts plus the Supervisor merge rule. Each reviewer works from the frozen target packet only. No reviewer sees author reasoning, prior verdicts, or other reviewers' conclusions.

Run all seven unless the user explicitly narrows scope. Do not merge roles to save effort. A role with no relevant evidence returns no findings, states what it inspected, and names its coverage limit.

## Code quality

Check defects, regressions, and maintainability costs in the assigned diff. Compare intent with behavior. Inspect the diff plus enough callers, tests, contracts, and configuration to establish impact. Defer to the author when approaches are equally valid. Recommend an alternative only for a concrete correctness, comprehension, performance, or maintainability cost.

Report only actionable findings with file and line, trigger, impact, and focused remedy. State open questions and validation limits. If no findings remain, say so.

## Architecture

Check dependency direction, layer separation, boundary data, and misplaced rules where the diff shows them. Trace source dependencies through imports, signatures, types, data formats, and wiring. Keep runtime control direction distinct from source dependency direction.

Report a finding only for demonstrated coupling, leakage, or testability cost in the changed behavior. Do not demand a fixed folder shape or a pattern for hypothetical reuse. If the scope has no architectural surface, report no findings with that limit.

## Naming

Check names that mislead, hide intent, or obstruct review in the assigned diff. Report a name only with a concrete comprehension cost and a better alternative tied to that cost.

Naming preference, style taste, or schema taste alone is not a defect. If names carry no demonstrated cost, report no findings.

## Bug hunting

Find concrete defects the change introduces. Trace changed behavior from realistic inputs and existing contracts. Check boundaries, errors, cancellation, state, concurrency, and resource ownership. Follow affected callers far enough to show failure.

Report a finding only with introduced cause, trigger, governing requirement, evidence, impact, and smallest remedy condition. Continue through the remaining diff after a find.

Exclude pre-existing bugs, intentional behavior changes, hypothetical inputs outside the contract, naming taste, and architecture suggestions without a concrete failure. Missing tests alone do not prove a bug. A passing test does not cover an untested path. If expected behavior is unresolved, report the uncertainty instead of inventing a requirement.

## Logic

Reconstruct the proposed change as goal, necessary premises, and conclusion. Ask whether the change accomplishes its actual goal. Identify each premise the conclusion needs. Seek realistic disproof with concrete inputs, states, or sequences from the code and contracts.

Reject preference-based naming or schema demands, invented requirements, conclusions without evidence, and unexecuted hypothetical failures. Preserve unresolved scope questions as open questions rather than proven bugs.

Report each logic break with the failed premise, the disproving scenario, the evidence, and the smallest correction condition. If the reasoning holds, say what was checked and what could still disprove it.

## Counterexamples

Attack the change with realistic adversarial cases. Use boundary values, empty and partial states, error and retry paths, concurrent use, and caller misuse that the contract permits. Prefer cases the existing tests do not cover.

Each counterexample needs concrete steps, the governing contract, observed or derived behavior from the code, and why it breaks the claim. Label unexecuted scenarios as unexecuted. Do not present an unrun scenario as a demonstrated failure. Keep scope questions open when the contract does not settle them.

## Validation audit

Check what the available tests and validation evidence actually establish for the changed behavior. Name each relevant check, its result, and the behavior it covers. Name uncovered behavior and evidence gaps.

Missing tests alone are not a defect. Do not invent a requirement to create a finding. Do not claim live behavior was verified unless the reviewer actually checked it under the authorized verifier rule in `SKILL.md`.

## Supervisor merge

Verify each role report against the frozen identity and hashes before merging. Discard results bound to a stale target. Group concordant findings by verified cause, not by vote. Distinguish accepted, rejected, unresolved, and out-of-scope candidates. Preserve dissent and incomplete gates. Record exact checks, exact code state, limitations, and a clear recommendation. Silence or an unavailable review never counts as approval. The Supervisor verifies and merges; it does not vote or repair.
