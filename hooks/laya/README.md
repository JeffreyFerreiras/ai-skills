# Laya routing

## Disabled

Laya hooks and subagent routing are disabled in the shared agent guidance. Do not
install, start, or invoke either integration or its OpenCode fallback unless the
user explicitly asks to re-enable it. The setup instructions below are retained
for that explicit request. The example config keeps both routing flags off.

To disable an existing installation, set both `enabled` and
`subagent_routing_enabled` to `false` in its runtime `config.json`. Remove the
managed Laya routing section from the active Codex instructions and remove only
Laya handlers from `UserPromptSubmit` and `SessionStart` in `hooks.json`, preserving
other guidance and handlers. Run `manage.ps1 Stop` to stop the local server.
`manage.ps1 Disable` changes only `enabled`; it does not disable subagent routing.

## Active subagent routing for normal chats

Install the subagent integration to have the primary agent classify bounded work
packets. Confident selections use a Codex `spawn_agent` call; uncertain results
use a separate OpenCode worker running `opencode-go/deepseek-v4.1-flash`:

```powershell
python hooks/laya/install.py --model-path D:\models\Laya\laya-english --integration subagents --skip-dependencies
```

Omit `--skip-dependencies` for a fresh runtime. This installs a managed section
from [subagent-instructions.md](subagent-instructions.md) into the active global
Codex `AGENTS.md` (or a nonempty `AGENTS.override.md`), preserving other guidance.
New chats load that section; existing chats need to read it explicitly. The
integration applies outside the engineering graph. It does not register prompt
hooks, change their trust, or change the main chat model/provider.

The primary calls [route_subagent.py](route_subagent.py) with JSON on stdin:

```powershell
@{ task = 'Find the files defining the Laya hook installer tests; return paths only.' } |
    ConvertTo-Json -Compress |
    & "$HOME/.codex/laya-hook/.venv/Scripts/python.exe" "$HOME/.codex/laya-hook/route_subagent.py"
```

| Packet or result | Runtime | Model | Effort |
| --- | --- | --- | --- |
| File finding | Codex spawn | `gpt-6-luna` | low |
| Bounded reading | Codex spawn | `gpt-6-luna` | medium |
| Routine coding | Codex spawn | `gpt-6.1-sol` | high |
| Architecture, difficult diagnosis, security or concurrency | Codex spawn | `gpt-6-astra` | high |
| Uncertain Laya result | OpenCode CLI | `opencode-go/deepseek-v4.1-flash` | No effort override |

`status: selected` includes `spawn` fields for model, effort, agent type and a
fresh context. The parent must use those fields, supply a complete work packet,
wait for the result and validate it. User overrides and required fixed-role
plans take precedence; coding cannot go to a read-only Luna packet. Children
must not route themselves again. Explicit spawn settings are supported by
[Codex subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents).

The English service starts on demand after a refused connection or connection
timeout, in a hidden process. The manager checks process ownership and the port
before starting. Startup is bounded to roughly 35 seconds including the
management command and request timeouts. Warm requests use the two-second limit.
Scores below 0.60, a winning margin below 0.15, truncation, malformed input or
service failure return `status: fallback`, without spawn settings. These cutoffs
are conservative heuristics, not calibrated accuracy guarantees.

Current uncertainty handling is a user-configured fallback: only the combination
`status: fallback` and `reason: uncertain` delegates the same bounded packet to
OpenCode DeepSeek. It does not change the router's result into a selection or
supply Codex spawn settings. Other fallback reasons, including disabled routing,
truncation, malformed input, service failure and unavailable Codex models, remain
in the parent. Explicit user choices, required fixed-role plans and
higher-priority delegation restrictions still take precedence.

Use the `opencode-muse-spark` skill's separate-process workflow with the explicit
model override `opencode-go/deepseek-v4.1-flash`, the `build` agent and `--auto`
(unless manual approval was requested). Resolve OpenCode, check its version and
confirm this exact ID in `opencode models` before launching; do not install it,
invent an alternative ID, set a DeepSeek effort, or send it to Codex
`spawn_agent`. Pass a PowerShell here-string prompt directly to
`opencode run --model opencode-go/deepseek-v4.1-flash --agent build --auto --dir
<absolute-workspace>`. Include the workspace, allowed paths and read/write intent,
preservation rules, acceptance criteria, validation, stop condition and the
no-recursion warning from [subagent-instructions.md](subagent-instructions.md).
Automatic approval does not expand the authorized scope or override a denial.

Supervise the same CLI process to terminal state, retaining its output and exit
status. Never duplicate an unresolved run after interruption or timeout. Inspect
the scoped diff and repository status, flag unexpected paths, and independently
run narrow checks before using its results; verify an unchanged workspace for
read-only work. Disclose unavailable CLI/model or failed execution and continue
in the parent. Report the actual model, exit status and validation. The managed
instructions contain the complete prompt and launch example.

Set `subagent_routing_enabled: false` in runtime config to disable this path,
then use `manage.ps1 Stop` to unload the service if desired. The existing
`enabled` switch controls only the separate prompt hook. Selection records use
`event: SubagentSelection`, a decision ID and `executed: false`: execution
requires an actual Codex spawn or a supervised OpenCode process, and the primary
must report the applicable execution result separately.
This is an instruction-driven dispatch workflow, not host-enforced interception.

The October 21, 2026 live smoke test selected Luna for a file search and the
parent actually launched that Luna subagent, which found the installer tests.
Across six synthetic packets, three returned a selection (file finding, reading,
routine coding) and three fell back as uncertain (another lookup, difficult
debugging, security). This small test does not establish general routing accuracy;
the Astra mapping has unit coverage but no successful live Astra dispatch in that
test. The cold call took about 15 seconds; warm classification took 78-216 ms,
excluding subagent startup and execution.

## Optional advisory prompt hook

A Windows Codex hook that asks a local English Laya checkpoint to classify the
latest prompt as `direct`, `investigate`, `implement`, or `design`. In advisory
mode, a synchronous `UserPromptSubmit` handler returns fixed routing guidance in
`hookSpecificOutput.additionalContext` before the model continues. It also logs
the result. Observation mode logs only.

The suggestion does not grant permissions, authorize delegation, change models,
or replace an approved engineering-graph plan. It reads no code or chat history.
The model can misclassify requests, so the agent checks it against user intent.

## Install

Requires Windows, Python 3.12+, PowerShell 7, and an already downloaded English
[Laya checkpoint](https://huggingface.co/convaiinnovations/laya). The installer
does not download or relocate model weights. The original installation used
revision `7b928d828b7b0e022f929d9bd2e44165aa270148`; use a known revision when
reproducing it. Runtime Laya is pinned in [requirements.txt](requirements.txt).

From the repository root, preview first:

```powershell
python hooks/laya/install.py --model-path D:\models\Laya\laya-english --dry-run
python hooks/laya/install.py --model-path D:\models\Laya\laya-english
```

The default destination is `$CODEX_HOME/laya-hook`, or `~/.codex/laya-hook` when
`CODEX_HOME` is unset. Override it with `--codex-home`. `--powershell` accepts a
PowerShell 7 executable when `pwsh` is absent from PATH. New environments are
isolated; `--reuse-system-packages` deliberately reuses an existing compatible
PyTorch/CUDA installation without changing global packages. `--skip-dependencies`
updates an existing runtime without changing its environment. The operator is
responsible for its dependencies when using that flag.

The installer copies only the runtime scripts and this README, creates a local
token if missing, and merges the two Laya handlers into `hooks.json`. Unrelated
hooks, existing tokens and logs, custom configuration keys, and existing mode,
device, port, and enabled state are preserved. Explicit `--mode`, `--device`, and
`--port` options override those fields. The required `--model-path` selects the
checkpoint location for this installation. No processes are started or stopped.

Review and trust the definitions through Codex's `/hooks` interface. Installation
does not modify trust or bypass it. Reload a session if newly installed hooks are
not picked up. [Codex hook documentation](https://learn.chatgpt.com/docs/hooks).

## Operate

```powershell
& "$HOME/.codex/laya-hook/manage.ps1" Start
& "$HOME/.codex/laya-hook/manage.ps1" Status
Get-Content "$HOME/.codex/laya-hook/routing.jsonl" -Tail 5
& "$HOME/.codex/laya-hook/manage.ps1" Disable
& "$HOME/.codex/laya-hook/manage.ps1" Stop
```

Use your chosen runtime path if you changed `--codex-home`. `Enable` restores
routing. Set `mode` in local `config.json` to `observe` for logging only or
`advisory` for model-visible guidance. Stop the server before upgrading a running
installation, then start it afterward to load the updated server script.

A separate asynchronous `SessionStart` hook requests server startup on startup,
resume, clear, and fork. It uses a hidden window and a mutex to avoid duplicate
servers. A cold model may not be ready for the first prompt; the hook then fails
open. Automatic prompt context delivery has been observed in Codex desktop;
automatic cold server startup has not been verified end to end.

The server binds only `127.0.0.1`, requires a locally generated bearer token, and
loads local weights offline. Its cache defaults beside the model directory;
override `cache_path` in runtime configuration if necessary. `device: auto` uses
upstream device selection; `cpu` and `cuda` request those devices. Check `Status`
for the actual device because upstream can fall back to CPU.

## Limits and logs

The English checkpoint has a 512-token window shared by the question and prompt.
Truncated requests and prompts longer than 12,000 characters return no guidance.
HTTP has a two-second timeout; the prompt hook has a four-second host timeout.
Failures return no context and do not block the user prompt. Hook context is
capped at 256 approximate tokens.

`routing.jsonl` records time, a hashed session identifier, prompt length, route
probabilities, truncation, elapsed time, and whether context was returned. It
contains no raw prompts, code, transcripts, or tokens. Logging stops at about
10 MiB; concurrent appends can exceed the limit by a few records. Move the log
aside to start another collection. Base-checkpoint probabilities are not
calibrated for engineering routing.

Keep runtime configuration, tokens, logs, PID files, virtual environments, model
weights, and caches outside this source directory. This is a hook integration,
not a discoverable skill; skill-profile synchronization does not deploy it.

## Verify and remove

```powershell
python -m unittest discover -s hooks/laya -p "test_*.py" -v
python -m unittest discover -s tests -p test_laya_install.py -v
python -m compileall -q hooks/laya
```

The tests use temporary runtime directories and mock inference; they need no
model download, GPU, or service. A live check should additionally confirm a new
advisory log entry and actual `additionalContext` delivery to the agent. A
`context_returned` log flag alone does not prove host delivery.

To uninstall, disable the hook and stop its server. Remove only the Laya handlers
from `UserPromptSubmit` and `SessionStart` in your user-level `hooks.json`.
Preserve unrelated handlers. Local data and model weights can be retained.
