<!-- laya-subagents:start -->
## Laya subagent routing

The user requests model-routed subagents for bounded work in normal Codex chats.
Only the primary routes work. Handle simple conversation, status replies, and
one-command verification directly.

1. Describe the actual bounded work packet, including relevant conversation
   context, complexity, risk, and read/write intent. Use PowerShell's
   `ConvertTo-Json` to encode a hashtable with a `task` string and pipe it to
   `& '__LAYA_PYTHON__' '__LAYA_ROUTER__'`. Keep it under 4,000 characters.
   The client reuses the resident LayaCodex service; it never starts a server.
2. For `status: selected`, use the returned `spawn.model`,
   `spawn.reasoning_effort`, `spawn.agent_type`, and `spawn.fork_turns` in an
   actual `spawn_agent` call. Add a task name and a self-contained work packet
   with workspace paths, constraints, ownership, and acceptance criteria.
   Model overrides require `fork_turns: none`. Map only supported equivalent
   fields if the tool schema differs. Prompt text alone does not switch models.
3. File finding uses GPT-6 Luna low; bounded reading uses Luna medium; routine
   coding uses GPT-6.1 Sol high; architecture, difficult diagnosis, security,
   and concurrency use GPT-6 Astra high. Reject incompatible selections:
   never route code writes to Luna or downgrade complex work. Explicit user
   choices, fixed-role plans, and higher-priority restrictions take precedence.
4. If routing fails, is uncertain, truncates, or selects an unavailable model,
   continue in the parent and briefly disclose the fallback. Do not retry in a
   loop or claim Laya selected the fallback. Do not bypass delegation limits.
5. Workers own named files, must preserve others' changes, and must be told
   they are not alone in the codebase. Read-only packets prohibit edits.
   Every child packet must say: "You are already the routed subagent;
   perform this packet directly and do not invoke Laya or spawn more agents."
6. Wait for the result and validate it. Report the model actually requested
   and any rejected selection or fallback when relevant. `executed: false`
   and a decision ID record selection only; they do not prove a subagent ran.
   Escalate failed or unexpectedly complex work with a revised Sol/Astra packet.

This selects subagent models and effort. It does not change the main chat's
model or effort. The separate advisory prompt hook is optional.
<!-- laya-subagents:end -->
