---
name: code-quality
description: Elevate selected code to expert-level quality. Use when the user wants a thorough review and improvement of code across clarity, structure, safety, performance, consistency, and maintainability — including a Clean Code / SOLID lens (Uncle Bob).
---

# Code Quality

## Purpose
Elevate the selected code to expert-level quality.

## Instructions
Evaluate and improve the code with these priorities:

- Clarity: remove noise, simplify logic, improve naming.
- Structure: enforce clean architecture, modular design, and separation of concerns.
- Safety: eliminate hidden bugs, edge cases, and brittle patterns.
- Performance: optimize critical paths without premature micro-optimizations.
- Consistency: follow idiomatic patterns for the language and framework.
- Maintainability: reduce duplication, improve readability, tighten interfaces.

## What would Uncle Bob say?
Consider the code through a Clean Code / SOLID lens (Robert C. Martin):

- **Single Responsibility**: Does each function, class, or module do one thing well? Would Uncle Bob say it's too big or doing too much?
- **Names**: Are names revealing intent? Would he ask "what does this variable/function actually do?" and get a clear answer from the name alone?
- **Functions**: Are they small, focused, and doing one level of abstraction? Any "doing X and then Y" that should be split?
- **Dependencies**: Are dependencies pointed inward (e.g. high-level policy not depending on low-level details)? Is dependency injection used instead of hidden singletons or globals?
- **Testability**: Could a unit test exercise this in isolation without mocks everywhere or hidden side effects? Is the design easy to test?
- **Duplication**: Any "rule of three" or copy-paste that should become a single, well-named abstraction?
- **Side effects**: Are I/O, mutation, or global state isolated and explicit, or scattered and surprising?

Call out concrete improvements that would make Uncle Bob nod, and fix any violations in the final code.

Return the final code and a brief summary of key improvements.
