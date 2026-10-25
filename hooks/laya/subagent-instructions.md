<!-- laya-subagents:start -->
## Laya subagent routing

The user requests model-routed subagents in ordinary Codex chats, including work
outside the engineering graph. At the primary agent only, delegate bounded file
finding, reading, and coding packets using this procedure. Simple conversation,
status replies, and one-command verification do not need a subagent.

1. Before delegating, send a concise description of the actual work packet to
   the local router. Include relevant complexity, risk and read/write intent.
   Use PowerShell's ConvertTo-Json to encode a hashtable with a `task` string,
   and pipe it to `& '__LAYA_PYTHON__' '__LAYA_ROUTER__'`.
   Keep task descriptions under 4,000 characters. The English classifier starts
   its local service on demand; the first call can take around 30 seconds.
2. If stdout has `status: selected`, use its `spawn.model`,
   `spawn.reasoning_effort`, `spawn.agent_type`, and `spawn.fork_turns` in the
   actual spawn_agent call. Add a task name and a self-contained work packet
   containing paths, constraints and acceptance criteria. Do not merely log or
   repeat the suggestion. Use a fresh context (`fork_turns: none`) for model
   overrides. When the spawn tool uses a different schema, map only supported
   equivalent fields; never claim a model change from prompt text alone.
3. File finding uses GPT-6 Luna low; bounded reading uses Luna medium; routine
   coding uses GPT-6.1 Sol high; complex reasoning uses GPT-6 Astra high. Check
   the classification against the packet: code writes must never go to Luna,
   and security, concurrency or architectural work must not be downgraded.
   Explicit user model/effort choices and required fixed-role graph plans take
   precedence. Reject an incompatible selection and state why.
4. Only when stdout has `status: fallback` and `reason: uncertain`, delegate
   the same bounded packet to a separate OpenCode CLI process with the exact
   model `opencode-go/deepseek-v4.1-flash`. This is the user's configured fallback,
   not a confident Laya selection. Follow the `opencode-muse-spark` skill's
   process with this explicit DeepSeek model override; do not pass this model to
   Codex `spawn_agent` or invent a DeepSeek reasoning-effort setting. Explicit
   user choices, required fixed-role plans and higher-priority delegation rules
   still take precedence.

   Before launching, inspect repository status, resolve `opencode` from PATH,
   check `opencode --version`, and check `opencode models` for that exact model.
   If resolution, either command or the exact model is unavailable, disclose it
   and continue in the parent without installing or substituting a model.
   Give the worker an explicit absolute workspace, allowed paths and read/write
   intent, preservation rules, acceptance criteria, validation and a stop
   condition. Include the child warning from step 5. Use a PowerShell here-string
   for prompt data and direct arguments, replacing the example placeholders:

   ```powershell
   $deepseekPrompt = @'
   ROLE: You are a delegated DeepSeek worker. Work only on this bounded packet.
   GOAL: <same packet and observable outcome>
   WORKSPACE: <absolute workspace>
   SCOPE: <allowed paths and read-only or write intent>
   PRESERVE: You are not alone in the codebase. Preserve others' changes and everything outside scope.
   CONSTRAINTS: Read applicable AGENTS.md files. Do not commit, push, publish, install dependencies, or perform destructive cleanup.
   You are already the routed subagent; perform this packet directly and do not invoke Laya or spawn more agents.
   ACCEPTANCE: <testable criteria>
   VALIDATE: <narrow commands or evidence to inspect>
   STOP: Stop when the scoped result is complete or immediately on a blocker.
   RETURN: Outcome, changed paths, validation results and blockers.
   '@
   opencode run --model opencode-go/deepseek-v4.1-flash --agent build --auto --dir 'C:\absolute\workspace' --title 'DeepSeek: scoped Laya fallback' $deepseekPrompt
   ```

   `--auto` does not expand authorized scope or override denied permissions;
   honor an explicit request for manual approval. Supervise this same process
   until terminal state and retain its exit status/output. An interrupted or
   timed-out run has an unknown result: inspect process/session and workspace
   state, and never duplicate an unresolved run. Inspect status and the scoped
   diff, flag unexpected paths, and independently run narrow validation before
   using the result; verify no changes for read-only work. If the run fails,
   disclose the failure and continue in the parent. Report the actual model,
   exit status and validation. All other fallback reasons, including routing
   failure, truncation and unavailable Codex models, continue in the parent with
   a brief disclosure. Do not retry in a loop or claim Laya selected a fallback.
5. Workers own explicitly named files, must preserve others' changes, and must
   be told they are not alone in the codebase. Read-only packets must prohibit
   edits. Every child packet must say: "You are already the routed subagent;
   perform this packet directly and do not invoke Laya or spawn more agents."
6. Wait for the child result, validate it appropriately, then use its findings
   or edits in the parent response. Report the model actually requested and
   any rejected selection/fallback when relevant. A classifier log alone does
   not prove a subagent ran. Escalate failed or unexpectedly complex work to
   Sol/Astra with a revised packet; do not repeatedly retry a failing Luna job.

This selects subagent models. It does not switch the main chat model. The
separate advisory prompt hook is not required for this procedure.
<!-- laya-subagents:end -->
