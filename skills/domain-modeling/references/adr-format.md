# ADR Format

Use the project's ADR location and numbering. Otherwise create `docs/adr/` lazily and use `0001-short-slug.md`, incrementing the highest existing number. Check that the new path is unused before writing.

A short decision record is enough:

```markdown
# Use domain events between Ordering and Billing

Ordering and Billing need independent availability. We chose domain events over
synchronous calls to avoid coupled outages, accepting delayed invoice creation
and the need to handle duplicate events.
```

Include the context, decision, reason, and meaningful tradeoff. Do not invent rejected options or imply acceptance when the decision remains a proposal.

Add status, considered options, or consequences only when useful or required by the project. When revisiting a decision, preserve its history and link the replacement under the project's convention.

Suggest an ADR only for a decision that is hard to reverse, surprising without context, and driven by a real tradeoff. A lock-in technology choice, context ownership boundary, or deliberate architectural deviation may qualify; its category alone is insufficient. An explicit request to record a decision overrides this suggestion threshold.
