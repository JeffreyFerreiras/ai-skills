# Resident Laya service and Codex hook

The [hook template](../hooks/laya-hooks.json) runs
[`scripts/laya-hook.py`](../scripts/laya-hook.py) on `UserPromptSubmit`. This small
standard-library client sends the current prompt to a resident offline CUDA model.
It never loads weights, downloads packages, or starts a server during a prompt.
The recommendation is advisory. It cannot change the active Codex model or
guarantee that a coordinator will delegate.

## Windows service installation

Requires Windows, Python 3.12+, an existing CUDA-enabled Laya environment at
`C:\models\laya\runtime`, and English weights at `C:\models\laya\weights`.
The tested interactive runtime uses Laya 0.4.12 and Torch 2.9.1+cu128. Package and
weight installation is separate; the installer does not download either.

Run this from **PowerShell opened as Administrator**, from this repository:

```powershell
& .\scripts\install-laya-service.ps1
```

The [installer](../scripts/install-laya-service.ps1) copies the service and client
to durable `C:\models\laya` paths. It registers the real Windows service
**LayaCodex**, running as **LocalService**, with delayed automatic startup. Recovery
restarts it after 5, 15, then 60 seconds, with the failure count reset after 24
hours. It grants LocalService read/execute access to the runtime, weights, and
the virtual environment's base Python installation. It grants modify access only
to `C:\models\laya\service-data` for offline cache files and startup diagnostics.
It does not install a logon task. `-Root` and `-Port` override the defaults.

The service launches the real base Python executable directly, avoiding the
Windows virtual-environment redirector's child process. It loads packages from
`runtime\Lib\site-packages`. The native entry point uses Python's standard library and the Windows
Service Control Manager API, so no service wrapper package is required. The
service loads one model, performs warmup, then binds **127.0.0.1:8766**. Requests
run serially, with a queue of at most four waiting connections. `/health` returns
ready only after load, warmup, and binding succeed; its PID and completed-request
count help confirm reuse. No prompt or decision logs are written.

The installer rejects an occupied port and waits up to **120 seconds** for the
actual service's readiness, matching the health PID to the running SCM service.
Interactive CUDA success does **not** prove CUDA works under LocalService in
Windows Session 0. If startup fails, inspect `Get-Service LayaCodex` and the
Windows System event log and `service-data\startup-error.txt` (exception type and
OS error code only). Run the diagnostic command below interactively to
separate runtime/weights errors from service-account access errors:

```powershell
C:\models\laya\runtime\Scripts\python.exe -B C:\models\laya\laya-service.py
```

Stop the service first before running that command on the same port. The older
`serve.cmd` is a foreground standard Laya API, not a Windows service and not this
Codex routing API. The separate OpenCode router on port 8767 is unchanged.

Service management, from elevated PowerShell:

```powershell
Get-Service LayaCodex
Invoke-RestMethod http://127.0.0.1:8766/health
Stop-Service LayaCodex
Restart-Service LayaCodex
```

To update an existing installation, stop the service, copy the two current Python
scripts into `C:\models\laya`, and start it again. The installer refuses to replace
an existing service silently. To uninstall:

```powershell
Stop-Service LayaCodex
sc.exe delete LayaCodex
```

Disable or remove the Laya hook in `/hooks` as well. Runtime and weight files stay
on disk, and the installer-added LocalService read permission stays in place.

## Codex hook setup

Merge the template's `UserPromptSubmit` handler into the existing hook config;
preserve other handlers. Do not install duplicate project and profile copies.
The template resolves the Git root and supports repository subdirectories. For a
user profile installation on this machine, use this handler instead:

```json
{
  "type": "command",
  "command": "C:/models/laya/runtime/Scripts/python.exe -B \"C:/models/laya/laya-hook.py\"",
  "timeout": 5,
  "statusMessage": "Checking local Laya task advice"
}
```

Use `/hooks` to review and trust changed hook definitions. The installer does not
change trust hashes or disable trust. Local command hooks require local
orchestration; cloud-orchestrated ChatGPT Work threads do not run them. Open a new
local session if a running session does not discover the configuration.

## Routing policy and fallback

Laya answers fixed task-type and difficulty questions. Routine recommends
**gpt-6-luna**, standard recommends **gpt-6.1-sol**, and demanding recommends
**gpt-6-astra**. Debugging and architecture receive at least Sol. The hook asks the
coordinator to delegate substantive tasks once with `fork_turns=none`, provide the
worker full context, and keep verification responsibility. Workers must not route
again. Status questions and short followups stay with the coordinator. Explicit
user model choices, instructions against delegation, and higher-priority rules
take precedence.

This preserves the installed trial's task/difficulty policy. It replaces the
earlier repository prototype's six-category, 0.8-probability filter; neither has
calibrated accuracy evidence. This is experimental English-only advice. The
classifier sees the current prompt, not conversation history.

Prompts over **1,200 characters** are skipped rather than shortened. Truncated or
malformed model output abstains. The hook reads at most **1 MiB** of event input
and **8 KiB** of response data. The service accepts bodies up to **8 KiB** and
uses a **1-second socket timeout** for incoming requests. The client uses a
**0.75-second socket timeout** and Codex caps the full hook at **5 seconds**.
Socket timeouts are per operation, not a total wall-clock deadline. Server
errors, unavailable service, malformed input, and timeouts return `{}` and exit
successfully, leaving the current model in place.

Connections use fixed loopback HTTP, ignore proxy variables, and do not follow
redirects. The service accepts only a prompt at `/classify`; callers cannot
provide arbitrary model questions. The loopback endpoint has no authentication,
so other local processes can call it. Only prompt text is sent, never transcripts
or files. Set `LAYA_CODEX_PORT` in Codex's environment if the service uses another
port. Set `LAYA_CODEX_DISABLED=1` or disable the hook in `/hooks` to skip inference.

## Verification

```text
python -B -m unittest discover -s tests -p test_laya_hook.py -v
python scripts/build-docs.py --check
git diff --check
```

Tests use an actual loopback server with a fake classifier to check client/server
contracts, repeated requests, routing floors, malformed and oversized input,
truncation, disabled mode, timeouts, and fail-open behavior. They do not establish
model accuracy, Windows-service startup, Session 0 GPU access, or deterministic
delegation. Validate the installed service separately with `/health` and repeated
`POST /classify` calls.

- [Codex hooks](https://developers.openai.com/codex/hooks/)
- [Laya SDK](https://github.com/NandhaKishorM/laya)
- [Windows service entry points](https://learn.microsoft.com/windows/win32/services/service-entry-point)
