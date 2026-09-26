---
name: generate-unit-tests
description: Add or improve focused unit tests for behavior, edge cases, and failures. Use for test-writing, coverage improvements, or bug reproduction.
---

# Generate Unit Tests

## Workflow

1. Inspect the source, its callers, existing tests, test framework, naming conventions, and available coverage tooling.
2. Identify observable behaviors and risks before choosing test cases.
3. Add the smallest coherent tests that cover success, validation, failure, boundary, and regression paths relevant to the code.
4. Run the focused test target. Measure coverage when the project provides a practical command or the user requests a target.
5. Report what was covered, the command result, and any behavior that remains difficult to isolate.

## Test Design

- Use clear Arrange/Act/Assert structure when it improves readability; do not add ceremonial comments.
- Name tests after behavior and outcome.
- Keep tests deterministic and independent of network, wall-clock time, random state, and shared mutable state.
- Mock external boundaries and costly side effects, not internal implementation details.
- Reuse existing builders, factories, fixtures, and helpers. Add a focused helper when repeated setup obscures intent.
- Prefer meaningful assertions over maximizing assertion count.
- Mirror the repository's test structure and conventions.

Treat 95 percent coverage as a target only when the user or project requires it. Otherwise prioritize important behavior and mutation-resistant assertions over a numeric threshold.
