---
name: what-would-uncle-bob-do
description: >
  Apply Robert C. Martin's ("Uncle Bob") Clean Code and Clean Architecture principles.
  Use this skill when writing, reviewing, or refactoring code to ensure it follows
  SOLID principles, clean naming, small functions, TDD, and proper layering.
  Keywords: clean code, SOLID, TDD, refactor, architecture, Uncle Bob, Robert Martin,
  single responsibility, dependency inversion, naming, functions, tests.
---

# What Would Uncle Bob Do?

Apply the principles and practices of Robert C. Martin ("Uncle Bob") — author of
*Clean Code*, *Clean Architecture*, and *The Clean Coder*.

---

## Core Philosophy

> "The only way to go fast is to go well."

Code is read far more than it is written. Optimise for the reader, not the writer.

---

## Clean Code Rules

### Naming
- Use **intention-revealing names**. If you need a comment to explain a name, rename it.
- Classes and types: **nouns** (`UserRepository`, `ReservationService`).
- Methods and functions: **verbs** (`fetchReservation`, `calculateTotal`).
- Avoid encodings, prefixes, noise words (`data`, `info`, `manager`, `helper`).
- Boolean names should read as predicates: `isActive`, `hasPermission`, `canBook`.

### Functions
- Functions should do **one thing** and do it well.
- Keep functions **short** — ideally fewer than 20 lines.
- **One level of abstraction per function.** Never mix high-level orchestration with low-level detail in the same function.
- **No side effects** — a function named `get*` must not mutate state.
- Prefer **0–2 parameters**. More than 3 is a smell; consider a parameter object.
- **Command–Query Separation**: a function either does something (command) or answers something (query), never both.

### Comments
- **Don't comment bad code — rewrite it.**
- Good code is self-documenting. Comments are for *why*, never *what*.
- Delete commented-out code immediately.

### Classes
- Apply the **Single Responsibility Principle (SRP)**: one reason to change.
- Keep classes small and cohesive.
- Hide data; expose behaviour (encapsulation).

---

## SOLID Principles

| Principle | Rule |
|-----------|------|
| **S** — Single Responsibility | A class has one and only one reason to change. |
| **O** — Open/Closed | Open for extension, closed for modification. Use abstractions. |
| **L** — Liskov Substitution | Subtypes must be substitutable for their base types without breaking correctness. |
| **I** — Interface Segregation | Prefer many small, focused interfaces over one large general-purpose interface. |
| **D** — Dependency Inversion | Depend on abstractions, not concretions. High-level modules must not depend on low-level modules. |

---

## Test-Driven Development (TDD)

Follow the **Red → Green → Refactor** cycle strictly:

1. **Red**: Write a failing test that describes the next small piece of behaviour.
2. **Green**: Write the *minimum* production code to make the test pass. No more.
3. **Refactor**: Clean up — remove duplication, improve names — without changing behaviour.

### Test Rules
- Tests must be **F.I.R.S.T**: Fast, Independent, Repeatable, Self-validating, Timely.
- One assert (or one concept) per test.
- Test names describe **behaviour**, not implementation: `login_withInvalidPassword_returnsUnauthorized`.
- Never skip a test; fix the code or delete the test — never comment it out.
- Test code is **production-quality code**. Refactor it just as rigorously.

---

## Clean Architecture (Boundaries)

```
Entities (Domain)
    ↑
Use Cases (Application / Services)
    ↑
Interface Adapters (Controllers, Presenters, Gateways)
    ↑
Frameworks & Drivers (DB, UI, Web, External APIs)
```

- **Dependency Rule**: source-code dependencies must point *inward* only.
- The domain layer knows nothing about databases, frameworks, or UI.
- Cross-boundary data structures contain only primitives or simple DTOs — never domain entities.
- Defer framework decisions as long as possible; they are details, not architecture.

---

## Error Handling

- Use **exceptions** for exceptional conditions; don't return `null` or error codes.
- Define application-specific exception types where appropriate.
- Never swallow exceptions silently.
- Handle errors at the boundary — not scattered through business logic.

---

## Practical Checklist (ask before every commit)

- [ ] Does every name reveal its intent?
- [ ] Does every function do exactly one thing?
- [ ] Are there any comments that should be code instead?
- [ ] Does every class have a single reason to change?
- [ ] Does everything depend on abstractions, not concretions?
- [ ] Is every new behaviour covered by a test written *before* the code?
- [ ] Is there any duplication (DRY)?
- [ ] Is the code at a consistent level of abstraction?
- [ ] Have I left the codebase cleaner than I found it (Boy Scout Rule)?

---

## Examples

### Bad
```
// Gets user data and does stuff
proc(u):
  d = db.query("SELECT * FROM users WHERE id = " + u)
  if d != null:
    sendEmail(d.email, "Hello " + d.name)
    db.query("UPDATE users SET last_login = NOW() WHERE id = " + u)
```

Problems:
- Cryptic names (`proc`, `u`, `d`)
- Mixed levels of abstraction (orchestration + SQL + email in one function)
- Side effects not obvious from the name
- Untestable (hard-coded dependencies)

### Good (Uncle Bob style)
```
notifyUserOnLogin(userId):
  user = userRepository.findById(userId)
  emailService.sendLoginGreeting(user)
  userRepository.recordLogin(userId)
```

- Intention-revealing name
- Single level of abstraction
- Dependencies injected (testable)
- No raw SQL, no string concatenation
- Each collaborator has a single responsibility