---
name: grilling
description: Interview the user to stress-test a plan, decision, or idea. Use when asked to grill their thinking or resolve choices through questions.
---

# Grilling

Reach a shared understanding of the requested decision through a focused interview. Challenge weak assumptions and explain tradeoffs without expanding into every possible future choice.

## Map the decision

Identify the outcome, scope, and constraints from the request and existing conversation. Ask about scope only when a missing boundary would change the interview.

Maintain a decision tree: each choice connects to the choices that depend on it. Track each relevant decision as:

- **Settled:** the user chose it or already supplied an explicit constraint.
- **Open:** it still needs a human choice or a fact.
- **Assumed:** a provisional working assumption, with its reason and consequence stated.

Preserve prior answers and authorization. Do not reopen a settled decision unless new evidence conflicts with it. If an answer changes a prerequisite, revisit only the dependent decisions it affects.

## Ask the next useful question

The frontier contains decisions whose prerequisites are settled. Select one frontier decision that most affects the requested outcome. Ask **one question at a time**, then wait for the user's answer before asking another.

Give short lettered options when there are useful alternatives. Put the recommended option first and explain its main tradeoff briefly. Allow the user to supply a different answer; use a free-form question when options would hide important possibilities.

Example:

> Which users should the first release serve?
>
> A. Existing customers (recommended). Tests demand with lower rollout risk.
>
> B. New customers. Tests acquisition, but needs onboarding work.

After each answer, update the tree and recompute the frontier. Do not ask a downstream question whose answer depends on an open choice. Keep the tree as working context; show only a brief status update when it helps the user follow progress.

## Find facts; leave choices to the user

Look up facts in available files, tools, or authoritative sources yourself. Do not ask the user to retrieve facts you can access. Use direct read-only investigation; delegation is optional only when available and authorized.

An unfinished lookup is an open prerequisite. Work on another ready branch while it is pending. If a required fact cannot be obtained, state what is missing and ask only for the information or choice the user must supply. Never label a guessed fact as settled.

The user owns material preferences and tradeoffs. Distinguish your recommendation from their decision. Make provisional assumptions visible, and resolve or explicitly accept those that could change the result before completion.

For an interview about domain concepts, relationships, or invariants, an available `domain-modeling` skill may help. Do not require it for unrelated decisions or assume it is installed.

## Finish within scope

Stop when the relevant tree has no unresolved decision or prerequisite needed for the requested outcome. An empty frontier alone is not completion: it can mean open prerequisites block the remaining branches. Do not grow new branches merely to prolong the interview.

Return a concise shared understanding: the settled choices, any explicitly accepted assumptions, and their material consequences. If the user ends early, distinguish settled choices from remaining open items and provisional assumptions; do not claim completion.

The interview does not authorize implementation, document writes, publication, or messages. Follow any separately authorized next action within its existing scope. Do not impose an extra confirmation step on work the user already authorized. If the request was only an interview, finish with the understanding in chat.

## Attribution

Adapted from Matt Pocock's [grilling skill](https://github.com/mattpocock/skills/blob/6fd947921b935b7e1e69293a200400f0fdd5c15f/skills/productivity/grilling/SKILL.md), pinned at `6fd947921b935b7e1e69293a200400f0fdd5c15f`. This adaptation keeps the decision tree and prerequisite frontier, replaces multi-question rounds with one question at a time, and bounds completion to the user's scope. The upstream MIT notice is in [LICENSE.txt](LICENSE.txt).
