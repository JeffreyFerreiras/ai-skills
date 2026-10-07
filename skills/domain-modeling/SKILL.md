---
name: domain-modeling
description: Sharpen project domain terms, resolve glossary contradictions, and record consequential decisions. Use when changing a domain model, glossary, or ADR, not merely reading vocabulary.
---

# Domain Modeling

Build a precise model of the project's concepts while discussing or designing them. Preserve the distinction between agreed domain rules, current implementation, and unresolved assumptions.

## Find the context

Use the project's existing glossary and ADR conventions. If a root `GLOSSARY-MAP.md` exists, follow it to the relevant context. Otherwise use an existing root `GLOSSARY.md`. Ask which context applies only when the topic is ambiguous.

Create documents lazily, when there is resolved content to record. Without an existing convention, use root `GLOSSARY.md` for the first agreed term and `docs/adr/` for the first qualifying decision. Do not invent multiple contexts or create empty document scaffolding.

## Sharpen the model

- Surface a term that conflicts with the glossary. Quote the relevant definition and explain the difference before treating the new meaning as settled.
- Replace vague or overloaded language with a proposed canonical term. Distinguish concepts such as Customer and User instead of silently calling both Account.
- Probe boundaries with concrete scenarios. For partial order cancellation, for example, ask whether cancellation affects an item, shipment, or entire order.
- Check relevant code and tests when a stated rule concerns current behavior. Cite contradictions, such as code that cancels the whole Order when the proposed rule permits partial cancellation. Do not silently prefer code or conversation; resolve which behavior is intended.
- Record agreed terms as they are resolved when document edits are in scope. In review-only or discussion-only work, provide proposed edits instead. Do not rename code or change behavior merely to fix vocabulary.

Use [the glossary format](references/glossary-format.md) when adding terms. Keep definitions concise and specific to the domain. Exclude implementation decisions, technical recipes, open questions, and general programming concepts from the glossary. Keep unresolved questions in the discussion or the project's existing planning artifact.

## Record consequential decisions

Offer an ADR only when **all three** hold:

1. Reversing the decision later has meaningful cost.
2. A future reader would find it surprising without context.
3. Genuine alternatives were weighed and a real tradeoff drove the choice.

Skip routine or easily reversed choices. Record a user-requested ADR even if it falls outside this suggestion threshold. Use [the ADR format](references/adr-format.md) when recording one, preserve existing numbering and status conventions, and distinguish proposed from accepted decisions. The skill does not grant authority to decide disputed requirements or implement the decision.

## Source

Adapted from Matt Pocock's [domain-modeling skill](https://github.com/mattpocock/skills/tree/6fd947921b935b7e1e69293a200400f0fdd5c15f/skills/engineering/domain-modeling), pinned at `6fd947921b935b7e1e69293a200400f0fdd5c15f`. Upstream files: `SKILL.md`, `GLOSSARY-FORMAT.md`, and `ADR-FORMAT.md`. Distributed under the [MIT license](LICENSE.txt).
