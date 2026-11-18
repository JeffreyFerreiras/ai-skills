---
name: recommend-model-effort
description: Recommend a model and reasoning effort for a task based on complexity, risk, and cost. Use when asked which model or effort to choose before starting work.
---

# Recommend Model Effort

Assess the task without performing it unless the user also asks for execution. Lead with one recommendation that names both the model and its reasoning effort.

## Workflow

1. Check the current host for model availability and effort options. Infer whether the user prioritizes latency or cost, balance, or quality. Default to balanced.
2. Use the requested model when verified. Otherwise, prefer the active model if it fits the task and priority, or choose one verified option. Use the model's actual name or ID. Consult current official documentation when exact effort mapping matters. Verify host availability separately.
3. Evaluate the work by its hardest reasoning demand, not by prompt length:
   - depth of dependent reasoning
   - ambiguity, novelty, or competing hypotheses
   - breadth of interacting components and constraints
   - difficulty of verifying the result
   - consequence of a plausible error
   - duration and autonomy required
4. Choose the lowest tier that reliably covers those demands using the ladder below.
5. Raise one tier when errors are costly, feedback is weak, constraints conflict, or adversarial edge cases matter. Lower one tier when the work is mechanical, tightly bounded, familiar, and cheaply verified. Do not count the same factor twice.
6. If the ideal tier is unavailable, use the nearest supported tier. Round up for quality-first or high-consequence work; round down for explicitly latency- or cost-sensitive work.

## Effort Ladder

- **None or minimal:** Use only when exposed, for direct extraction, formatting, classification, or deterministic transformations needing essentially no deliberation.
- **Low:** Use for simple, bounded, familiar tasks with obvious success criteria, little ambiguity, and fast verification. Examples include a small rename, a direct lookup, or a routine explanation.
- **Medium:** Use as the balanced default for ordinary non-trivial work with several steps or local tradeoffs. Examples include a conventional feature with tests, a focused code review, or analysis using well-understood methods.
- **High:** Use for complex debugging, design, refactoring, or analysis that requires reconciling several constraints, exploring plausible alternatives, or checking cross-component effects.
- **Xhigh:** Use for genuinely difficult, uncertain, long-horizon, or high-consequence work where deeper search and verification are likely to improve reliability materially.
- **Max:** Reserve for the hardest quality-first work where marginal reliability matters more than latency or token use and substantial exploration or verification can change the outcome. Do not choose it merely because a task is large.
- **Ultra:** Use only when the active host exposes it and current host-specific guidance establishes its meaning and appropriate use. Do not infer that it enables delegation or requires parallel work. If its semantics are unavailable, state the uncertainty and recommend another supported level from the task demands.

Task length, file count, or tool count alone never justifies high effort. Easy-to-test bulk work can remain low or medium. A short task can warrant xhigh or max when a subtle error would be costly and hard to detect.

Do not confuse reasoning effort with response length. Recommend verbosity separately only when asked.

## Response Format

Lead with one recommendation:

```text
Recommended model and effort: <model name or ID> · <supported level or labeled relative tier>
Why: <one sentence naming the decisive task signals>
```

If the requested model is unavailable or unverified, lead with its name and
status, then label the recommendation `Alternative model and effort` when
there is a verified alternative. If no suitable model can be verified, say so
in the model field. Whenever exact effort options are unknown, give a clearly
labeled relative effort tier.

Add at most one of these when material:

```text
Assumption: <missing constraint you inferred>
Use <adjacent level on the same model> instead if: <single boundary condition>
```

Ask one concise question only when missing information would change the model choice materially or shift the effort by two or more tiers. Otherwise, state the assumption and recommend directly.
