---
name: api-docs
description: Add accurate .NET XML documentation to C# APIs. Use when asked to document public members or repair missing XML comments.
---

# .NET API Documentation

## Workflow

1. Scope documentation to changed public members unless the user requests a broader surface.
2. Inspect usages, interfaces, implementations, validation, and tests before describing behavior.
3. Document the contract and business purpose rather than restating identifiers or syntax.
4. Run the narrowest documentation analyzer or build target available for the affected project.

## XML Documentation

- Use `<summary>` for purpose and observable behavior.
- Add `<typeparam>`, `<param>`, `<returns>`, and `<value>` where applicable.
- Add `<exception>` only for exceptions callers can meaningfully observe from the documented contract.
- Put detailed contract documentation on interfaces and use `<inheritdoc />` for implementations when the contract is unchanged.
- Use `<see cref="..." />` and `<paramref name="..." />` for navigable references.
- Preserve important nullability, units, ranges, side effects, ordering, thread-safety, and lifecycle constraints.
- Avoid empty boilerplate, guessed behavior, implementation details, and comments that merely repeat the member name.

Report the documented surface and validation result.
