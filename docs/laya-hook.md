# Laya routing: one resident service

Laya uses one **LayaCodex** Windows service, one routing policy, and two lightweight
clients: an explicit subagent selector and an optional advisory prompt hook.
This consolidates PR #43's active subagent routing with PR #44's resident service.
Neither path changes the main chat's model or reasoning effort.

## Routing policy

The shared [policy](../scripts/laya_routing.py) uses fixed work-packet questions:

| Work | Model | Effort |
| --- | --- | --- |
| File finding | `gpt-6-luna` | low |
| Bounded reading | `gpt-6-luna` | medium |
| Routine coding | `gpt-6.1-sol` | high |
| Architecture, difficult diagnosis, security, concurrency | `gpt-6-astra` | high |

The selected probability must be at least **0.60** and exceed every other score
by at least **0.15**. Missing scores, invalid scores, truncation, and uncertain
results return a fallback without spawn settings. The thresholds are heuristics;
there is no calibrated accuracy or cost-saving claim. This replaces the merged
service's separate task/difficulty policy.

The parent classifies the actual bounded work packet, including relevant
conversation context, complexity, risk, and read/write intent. It checks the
selection, launches the subagent with the returned settings, waits, and validates
the result. Code writes must never go to Luna, and complex work must not be
downgraded. Explicit user choices, fixed-role plans, and higher-priority rules
take precedence. Children must not route again.

This remains instruction-driven delegation. A `selected` response and decision
ID do not prove execution. The selector returns `executed: false`; only an
actual subsequent subagent launch establishes execution.

## Install the resident Windows service

Requires Windows, Python 3.12+, an existing CUDA-enabled Laya environment at
`C:\models\laya\runtime`, and English weights at `C:\models\laya\weights`.
The tested earlier runtime used Laya 0.4.12 and Torch 2.9.1+cu128. The installer
does not download packages or weights.

From an elevated PowerShell in this repository:

```powershell
& .\scripts\install-laya-service.ps1
```

The [installer](../scripts/install-laya-service.ps1) copies `laya-service.py`,
`laya-hook.py`, and `laya_routing.py` into durable `C:\models\laya` paths.
It registers **LayaCodex** under **LocalService** with delayed automatic startup
and recovery after 5, 15, then 60 seconds, resetting the failure count after 24
hours. It grants LocalService read/execute access to the runtime, weights, and
base Python, and modify access to `service-data` for offline caches and startup
diagnostics. `-Root` and `-Port` override defaults.

The real base interpreter runs directly, avoiding the Windows venv redirector's
child process. The service loads packages from `runtime\Lib\site-packages`,
warms one offline CUDA model, then binds **127.0.0.1:8766**. Requests are serialized
with at most four waiting connections. `/health` reports readiness, PID,
`protocol: laya-routing-v1`, and completed-request count. It writes no prompts
or decision logs.

The installer rejects existing services and occupied ports. It waits up to
**120 seconds**, matching the health PID with the running SCM service.
Tests with fake inference do not establish CUDA access under LocalService in
Windows Session 0. Inspect `service-data\startup-error.txt` and the Windows
System event log on startup failure. The diagnostic contains only exception
class and OS code.

## Install the Codex clients

The [profile installer](../hooks/laya/install.py) needs no model packages,
new environment, administrator rights, or running service:

```powershell
python hooks/laya/install.py --dry-run
python hooks/laya/install.py
```

The default `--integration subagents` installs clients in
`$CODEX_HOME/laya-hook` (or `~/.codex/laya-hook`) and updates only the
`laya-subagents` managed section in the active global `AGENTS.md` or nonempty
`AGENTS.override.md`. New chats load it. Existing chats must read the updated
instructions explicitly.

Use `--integration both` for explicit routing plus one optional prompt hook,
or `--integration hooks` for prompt advice alone. `--codex-home`, `--python`,
`--service-root`, and `--service-port` override installation defaults.
The Python executable runs only standard-library clients.

Example explicit selection after installation:

```powershell
@{ task = 'Find the Laya installer test files; return paths only. Read-only.' } |
    ConvertTo-Json -Compress |
    & python "$HOME/.codex/laya-hook/route_subagent.py"
```

A successful response contains `spawn.model`, `reasoning_effort`,
`agent_type`, and `fork_turns: none`. Use those fields in the actual spawn
tool call, with a complete packet and workspace path. Unsupported selections
fall back to the parent. The client never starts a service or loads a model.

The optional prompt hook uses the same policy but sees only the latest prompt.
Its output is advice, grants no delegation authority, and asks the parent to
classify the actual packet before any authorized delegation. There is no second
model/effort selection policy. Review changed definitions through **/hooks**;
the installers never change trust hashes.

The [repository template](../hooks/laya-hooks.json) supports nested paths but
must be merged manually. Use one profile or project registration; the profile
installer cannot remove a separate project-local registration. Local command
hooks need local orchestration, not cloud orchestration.

## Migrate the two previous installations

Update the resident service before switching clients. From elevated PowerShell,
using your actual service root:

```powershell
Stop-Service LayaCodex
Copy-Item -LiteralPath .\scripts\laya-service.py -Destination C:\models\laya\laya-service.py -Force
Copy-Item -LiteralPath .\scripts\laya-hook.py -Destination C:\models\laya\laya-hook.py -Force
Copy-Item -LiteralPath .\scripts\laya_routing.py -Destination C:\models\laya\laya_routing.py -Force
Start-Service LayaCodex
Invoke-RestMethod http://127.0.0.1:8766/health
```

For the earlier on-demand installation, stop its old server **before** migrating
the profile, using its existing `manage.ps1 Stop`. That manager belongs to the
legacy implementation and must not manage the resident Windows service.

Then run `python hooks/laya/install.py` (or choose `--integration both`).
The installer removes recognized legacy `SessionStart` handlers and both older
profile prompt handlers, then installs at most one new prompt handler. It
preserves unrelated handlers and group metadata, profile instructions outside
the managed section, custom configuration keys, model weights, tokens, logs,
old scripts, and existing environments. It does not stop processes or delete
runtime assets. No replacement backups are created.

The resident endpoint defaults to **8766**, stored as `service_port`. The
legacy `port: 18147` key is retained but no longer controls the clients.
Use `--service-port` for a nondefault resident port. Existing clients must be
updated together with the service; mixed protocol versions fail open.

The old `--model-path`, `--skip-dependencies`, and `--device` installer
options are retired. Model/runtime installation belongs to the resident service;
the profile installer only installs clients. No `SessionStart` hook or
on-demand server is installed. Other routers, including OpenCode on port 8767,
are outside this migration.

## Limits, privacy, and fallback

The English classifier receives only the supplied prompt or task string, never
files or transcripts. Include needed context in a concise task description.
Prompt advice skips inputs over **1,200 characters**; explicit packets allow
**4,000 characters** and at most **32 KiB** of stdin. Model-window truncation
still abstains even within those limits.

The service accepts at most **32 KiB** per HTTP body and uses a **1-second**
incoming socket timeout. Clients bound response reads to **8 KiB** and prompt
events to **1 MiB**, with a **0.75-second** socket timeout. Socket timeouts apply
per operation. Codex caps the prompt hook at **5 seconds**.
Malformed inputs, incompatible service output, service failures, and timeouts
return `{}` for prompt advice or `status: fallback` for explicit selection,
with a successful process exit and no spawn settings.

HTTP uses fixed loopback, ignores proxies, and does not follow redirects.
Callers cannot supply arbitrary model questions or model names. The resident
endpoint retains PR #44's unauthenticated loopback boundary; other local
processes can call it. PR #43's separate bearer-authenticated server is not
started by these clients. No raw prompts, tokens, or exception messages are
written to routing logs.

`LAYA_CODEX_PORT` overrides `service_port`. `LAYA_CODEX_DISABLED=1` disables
both clients. In client `config.json`, `subagent_routing_enabled: false`
disables selection; `enabled: false` or `mode: observe` suppresses prompt
advice. Observation mode does not write decision logs in this consolidated client.

## Verify and remove

```text
python -B -m unittest discover -s tests -p test_laya*.py -v
python scripts/build-docs.py --check
git diff --check
```

Tests use fake inference with a real loopback server. They cover both clients
sharing a process, model/effort mapping, probability validation, malformed
responses, input limits, timeouts, and migration that preserves unrelated data.
They do not establish real-model accuracy, live service upgrades, or Session 0
CUDA access. Verify the installed `/health` protocol and repeated requests
separately.

To uninstall, disable the clients, remove only the Laya prompt handler and
managed instruction section, then stop/delete the service from elevated
PowerShell. Runtime files, weights, and installer-added LocalService permissions
remain unless removed separately.

- [Codex hooks](https://learn.chatgpt.com/docs/hooks)
- [Laya SDK](https://github.com/NandhaKishorM/laya)
- [Windows service entry points](https://learn.microsoft.com/windows/win32/services/service-entry-point)
