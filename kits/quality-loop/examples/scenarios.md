# Example scenarios (stubs)

## 1. Bugfix on a service boundary
Orient with graph → fix with clean-code → validate → code-review → loop if needed.

## 2. Refactor for clarity (behavior preserved)
clean-code (+ clean-architecture-code if deps move) → tests must stay green → code-review focused on regressions.

## 3. Feature slice behind a clear interface
graph for touch points → implement → architecture review on the new boundary → code-review → PR.
