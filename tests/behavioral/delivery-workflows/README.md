# Delivery workflow scenarios

Use these cases for routing and next-action checks on full-send, full-stop,
babysit-pr, and their local dependencies. Follow the parent behavioral evaluation
protocol. Give a fresh selection session the relevant catalog and
`inputs.json` only; keep `expected.json` for an independent evaluator.

Have the session record each case's selected workflow, code/prose review route,
allowed mutations, readiness decision, and next action, with source evidence.
Use synthetic state only. Do not contact GitHub, alter profiles, or execute the
hypothetical delivery. Record actual file reads and any output artifact separately
from proposed actions.

Grade the observed decisions against the hidden expectations. These cases test
routing and stated next actions, not live CI watching, merge races, actual review
execution, or protection against unauthorized external writes. Those remain
unverified until an isolated execution test covers them.
