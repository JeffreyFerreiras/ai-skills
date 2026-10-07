# Ticket Format

Honor the project's template first. This fallback works as one Markdown file per ticket or as an issue body.

```markdown
# 02: Customer cancels an unshipped order

## What to build

A customer can cancel their own unshipped order and see its cancelled state.
Shipped orders remain unchanged and explain why cancellation is unavailable.

## Acceptance criteria

- [ ] The customer can cancel their own unshipped order and see its new state.
- [ ] The customer cannot cancel another customer's order.
- [ ] Cancelling a shipped order returns the agreed rejection and preserves it.
- [ ] Integration checks exercise cancellation, authorization, and rejection.

## Blocked by

01: Customer can view their own order and its fulfillment state

## Assumptions or open decisions

Cancellation applies to the whole order, as defined in the agreed glossary.
```

This is an illustrative slice, not a required product feature or domain rule. Use "None (can start immediately)" only when no unfinished ticket dependency or unresolved decision precondition prevents starting. If there are no ticket dependencies but a required decision remains open, write "No ticket dependencies; awaiting <decision>" and name it under implementation preconditions. Do not mark a blocked or undecided ticket as ready for implementation.

For published issues, replace local IDs with real tracker references. Add a parent reference only when the source is a parent issue and the linkage is authorized. Omit textual blockers when native links carry the same information.

For an expand-migrate-contract plan, make compatibility checks explicit on expand, batch checks on migrate, and the no-old-callers check on contract. If only a shared integration point can pass, record that constraint on each affected ticket.
