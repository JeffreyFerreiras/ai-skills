# Model catalogs

## Present, adjust, approve

The graph recommends assignments; it does not require its preferred models. Select the actual
harness catalog: `codex-astra` (default Codex preference), `codex` (Sol preference on Codex),
`claude`, or `cursor`. New Codex plans use catalog revision 6; Claude and Cursor remain on
revision 3. Verify the selected pair against the host's current exposed capabilities; catalog
entries do not establish account access.

| Catalog | Helpers and fixed research | Core roles | Architecture and review |
| --- | --- | --- | --- |
| codex-astra | gpt-6-astra / medium | gpt-6-astra / medium | gpt-6-astra / high |
| codex | gpt-6-sol / medium | gpt-6-sol / medium | gpt-6-sol / high |
| claude | claude-sonnet-5 / low | claude-opus-5 / medium | claude-opus-5 / high |
| cursor | gemini-3.8-flash / low | grok-4.7 / medium | grok-4.7 / high |

Core roles include Tech Lead, Senior Engineer, and Test Engineer. Architect, Code Reviewer,
Security Reviewer, and Release Operations Reviewer receive the review suggestion. Other advisory
and specialist nodes start with the core suggestion. Supervisor recommendation uses the review
suggestion; publication starts with the helper suggestion. Consolidation remains in the primary
thread. These task-adjustable defaults are user preferences, not evaluated quality/cost claims.
For Codex, the preview's `economy_fanout_option` names GPT-6 Luna / max. It is selectable for
the impact mapper, both fixed research nodes, publication, and bounded helpers. The helper
allowance must bind the economy pair separately; graph-node overrides do not change it.

Before initialization, present the actual execution sequence and relevant/conditional roles,
each model and effort, helper allowance, alternatives, assumptions, and availability gaps.
Invite changes and show the revised plan before approval. Use this optional field in task-brief
v2/v3 to record selections (node keys, not role-profile names):

```json
"model_overrides": {
  "tech_lead": {"model": "gpt-6-sol", "reasoning_effort": "medium"},
  "senior_engineer": {"model": "gpt-6-sol", "reasoning_effort": "high"},
  "supervisor_recommendation": {"model": "gpt-6-sol", "reasoning_effort": "high"},
  "impact_mapper": {"model": "gpt-6-luna", "reasoning_effort": "max"},
  "design_research_architecture": {"model": "gpt-6-luna", "reasoning_effort": "max"},
  "design_research_validation": {"model": "gpt-6-luna", "reasoning_effort": "max"},
  "publication_assignment": {"model": "gpt-6-luna", "reasoning_effort": "max"}
}
```

Each omitted entry retains its recommendation. Unknown nodes, malformed selections, inherited
worker models, unsupported harness/model/effort pairs, and cross-provider mismatches are rejected.
The preview lists `model_options` for deliberate selection. The supported set is separate from
the recommendation matrix, so GPT-6 Sol and GPT-5.6 Terra are selectable without changing
the Astra recommendation. Cursor also offers Gemini, Composer, Sonnet, Opus, Fable, and
GPT-5.6 Sol alternatives. Claude offers Sonnet, Opus, and Fable. Do not map an effort the
selected model does not expose: Grok 4.7 stops
at xhigh; Gemini 3.8 Flash stops at high. A newly available pair requires a reviewed catalog update,
not an arbitrary unchecked string. Such updates must preserve prior revision reconstruction.

With the runtime-home policy loaded:

```text
python <skill>/scripts/graphctl.py --repo <repo> plan --run-id <id> --task-brief <path> --host cursor
```

This performs bounded input validation and returns the unsigned-for-approval candidate plan without
creating state or dispatching agents. Edit the uninitialized task brief and repeat this preview
until selections are settled. Then initialize with the same brief/host/size and approve the exact
returned digest. Approval covers every possible assignment, including conditional roles.
Policyless planning stays conversational; this command does not bypass missing or invalid policy.

The brief is immutable immediately after initialization, even while approval is pending.
Changing selections then requires a new run and approval. Never edit stored task/plan JSON or
reset consumed budgets to obtain another retry. An unavailable selected model blocks only that
dispatch while the Supervisor proposes an alternative for approval. Never silently substitute.
Approval establishes permission, not runtime capability.

## Helpers and native roles

Prepare the [helper allowance](economy-helpers.md) using the preview's `helper_recommendation`
or an explicitly selected supported alternative. The allowance is separately hash-bound; a graph
node override does not rewrite it. Actual helper dispatch still requires exact observed availability,
host capability checks, scope, budgets, reservation, and settlement.

Codex TOMLs are installation defaults, not requirements on every harness. Claude and Cursor use
their native subagent configuration or a supported fresh-agent mechanism with the same role
contract and the approved model/effort. A pinned native role must not override the approved
selection. Do not claim that catalog support installs native roles or proves live delegation.

## Historical compatibility

The original HOST_MATRIX is frozen. Missing revisions preserve the historical class/size matrix;
explicit Claude revision 1 and Codex/Astra/Cursor revision 2 reconstruct their exact old defaults.
Revisions 3 through 5 retain their original recommendations, model options, bytes, digests, and approvals.
New Codex revision 6 plans recommend Astra or Sol at medium effort for helpers and fixed research,
and expose GPT-6 Luna / max as an economy fanout option. Older engines reject revision 6 rather
than reinterpret it.
Existing conditional reviewer-fanout policy and its budget weights are unchanged; its assignments
remain separately selected in the runtime-home policy, not through graph-node overrides.

## Source and evaluation scope

Selection guidance checked September 23, 2026:
[OpenAI models](https://developers.openai.com/api/docs/models),
[OpenAI pricing](https://developers.openai.com/api/docs/pricing),
[Artificial Analysis GPT-6 Luna max versus xhigh](https://artificialanalysis.ai/models/comparisons/gpt-6-luna-vs-gpt-6-luna-xhigh),
[Claude models](https://platform.claude.com/docs/en/models/overview),
[Claude effort](https://platform.claude.com/docs/en/build-with-claude/effort),
[Cursor Grok 4.7](https://cursor.com/docs/models/grok-4-7),
[Cursor Gemini 3.8 Flash](https://prod.cursor.com/docs/models/gemini-3-8-flash),
[Cursor subagent selection](https://prod.cursor.com/help/models-and-usage/available-models).
Native dispatch interfaces may expose model IDs and effort separately; do not invent suffixed aliases.

GPT-6 Sol and Luna have lower listed per-token prices than GPT-5.6 Sol and Luna, respectively;
GPT-6 Astra has a higher per-token price than GPT-5.6 Sol. Artificial Analysis reports stronger
benchmark results for GPT-6 Luna max than xhigh, with greater latency and token use. These are
configured recommendations.
Deterministic tests establish selection, binding, and compatibility, not model quality or live
Claude/Cursor operation. Use the authorized disposable
[behavioral evaluations](behavioral-evaluations.md) before claiming cross-harness execution,
savings, or reliability. Report unavailable telemetry rather than estimating it.
