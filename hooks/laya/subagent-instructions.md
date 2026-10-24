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
4. If routing fails, truncates, is uncertain, or requests an unavailable model,
   continue in the parent and disclose the fallback briefly. Do not retry in a
   loop or claim Laya selected the fallback. Respect any higher-priority rule
   that disallows delegation.
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
