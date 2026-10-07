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
- Derive expected values from the contract, a checked example, or an independent reference. Do not copy the production calculation into the assertion or generate an expected snapshot from the result under test. For example, assert that prices 10 and 5 total 15 rather than repeating the same summation loop.
- Mirror the repository's test structure and conventions.

Treat 95 percent coverage as a target only when the user or project requires it. Otherwise prioritize important behavior and mutation-resistant assertions over a numeric threshold.

## Requested Test-First Work

When the user requests test-first implementation, choose an observable behavior at an existing public boundary. Add one focused test, confirm that it fails for the intended reason, then make the smallest authorized implementation change that passes it. Repeat for the next behavior, using each result to guide the next test. Do not write a speculative batch of tests before learning from the first cycle.

This cycle applies when implementation is authorized. A request to add tests alone does not authorize production changes, and existing tested behavior need not fail first. Do not impose strict TDD or a separate approval gate on ordinary test-writing or refactoring work.
