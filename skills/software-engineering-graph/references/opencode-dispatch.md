# OpenCode Muse dispatch

Catalog revision 9 permits an approved Codex graph node or publication assignment to select
`opencode-go/muse-spark-1.3-contributor`. Its `dispatch_runtime: opencode-cli` is part of the
immutable plan. The ledger still coordinates attempts; it does not execute model calls.
The Supervisor is the only ledger operator and must launch and verify this external process.

Before claiming a Muse assignment, verify that `opencode` resolves, `opencode --version`
works, and `opencode models opencode-go` lists the exact model. Check that the selected
`reasoning_effort` is an available Muse variant using `opencode models opencode-go --verbose`.
Revision 9 recognizes `minimal`, `low`, `medium`, `high`, and `xhigh`; current CLI output and
account access still need verification at dispatch. If any check fails, do not claim or launch
the node. Propose a supported replacement through a new approved plan, never substitute one.

After the normal claim, make one bounded prompt from the claimed branch envelope and selected
role contract. Give the worker its goal, allowed paths and effects, input references, output
contract, validation or read-only condition, and stop condition. State that other agents may
work in the checkout and that unrelated changes must be preserved. Do not pass the ledger path,
operation IDs, unneeded files, secrets, or broad Git, publication, or cleanup authority.
The claimed envelope carries `dispatch_runtime: opencode-cli`; treat a missing or different
value for a Muse branch as a plan/envelope mismatch and stop.
Only the execution-plan-authorized Pull Request Engineer may receive the separately approved
publication effects. A Muse publication selection does not grant them by itself.

Use a separate OpenCode CLI session, the exact approved model and variant, and the intended
workspace. In PowerShell, pass the prompt as one argument rather than building a shell command
from its contents:

```powershell
$branchPrompt = @'
<bounded role and envelope prompt>
'@
opencode run --model opencode-go/muse-spark-1.3-contributor --variant xhigh --agent build --dir 'C:\path\to\worktree' --title 'Graph: bounded role' $branchPrompt
```

Replace `xhigh` with the approved variant if the plan selects another effort. Add `--auto` when the user's standing automatic
approval preference applies; it does not enlarge the role's approved effects. Check OpenCode's
actual permission behavior before relying on it. Prompt limits are cooperative, not filesystem
or tool confinement. For a read-only role, inspect status and the scoped diff afterward and
reject unexpected changes. For a writer, inspect all changed paths and run the approved focused
checks. A zero exit status or worker claim alone is not accepted evidence.

Wait for the same process to finish and retain its exit status and output. If it times out or is
interrupted, treat the outcome as unknown. Inspect the process/session and workspace before
retrying; do not launch a duplicate while the first may still run. Record the branch result
through the normal fenced ledger operation only after checking its output contract and evidence.
OpenCode session usage is not parsed by the Codex JSONL usage reducer. Report its token usage as
unavailable unless a separately supported accounting source is added; do not infer savings from
catalog prices or elapsed time.

Direct Evidence Scout and Validation Executor helpers have a separate allowance and register.
Muse is not accepted in that allowance. Adding a helper route would require verified per-child
confinement and a compatible register lifecycle.
