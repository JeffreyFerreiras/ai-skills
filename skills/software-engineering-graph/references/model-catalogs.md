# Model catalogs

## Present, adjust, approve

The graph recommends assignments; it does not require its preferred models. Select the actual
harness catalog: `codex-astra` (default Codex preference), `codex` (Sol preference on Codex),
`claude`, or `cursor`. New Codex plans use catalog revision 9; Claude uses revision 4 and
Cursor uses revision 5. Verify the selected pair against the host's current exposed capabilities; catalog
entries do not establish account access.

| Catalog | Helpers and fixed research | Core roles | Architecture and review | External economy option |
| --- | --- | --- | --- | --- |
| codex-astra | gpt-6.1-sol / low | gpt-6-astra / medium | gpt-6-astra / high | Muse Spark 1.3 / xhigh |
| codex | gpt-6.1-sol / low | gpt-6.1-sol / medium | gpt-6.1-sol / high | Muse Spark 1.3 / xhigh |
| claude | claude-sonnet-5-5 / low | claude-opus-5-5 / medium | claude-opus-5-5 / high | none |
| cursor | gemini-3.8-flash / low | grok-4.7 / medium | grok-4.7 / high | none |

For either Codex catalog, the preview also offers `opencode-go/muse-spark-1.3-contributor`
at `xhigh` as an external economy alternative. It is selectable at `minimal`, `low`, `medium`,
`high`, or `xhigh` for graph nodes. The recommendation table above stays unchanged. A Muse
selection is bound to `dispatch_runtime: opencode-cli`; follow the
[OpenCode dispatch procedure](opencode-dispatch.md), not a native Codex agent launch.
This does not add Muse to separately approved conditional reviewer fan-out.
Direct reusable helpers use a separate register and cannot select Muse in this revision.

Core roles include Tech Lead, Senior Engineer, and Test Engineer. Architect, Code Reviewer,
Security Reviewer, and Release Operations Reviewer receive the review suggestion. Other advisory
and specialist nodes start with the core suggestion. Supervisor recommendation uses the review
suggestion; publication starts with the helper suggestion. Consolidation remains in the primary
thread. These task-adjustable defaults are user preferences, not evaluated quality/cost claims.
For Codex, the preview's `economy_fanout_option` names GPT-6.1 Sol / low. It is recommended for
the impact mapper, both fixed research nodes, publication, and bounded helpers. The helper
allowance must bind the economy pair separately; graph-node overrides do not change it.

Before initialization, present the actual execution sequence and relevant/conditional roles,
each model and effort, helper allowance, alternatives, assumptions, and availability gaps.
Invite changes and show the revised plan before approval. Use this optional field in task-brief
v2/v3 to record selections (node keys, not role-profile names):

```json
"model_overrides": {
  "tech_lead": {"model": "gpt-6.1-sol", "reasoning_effort": "medium"},
  "senior_engineer": {"model": "gpt-6.1-sol", "reasoning_effort": "high"},
  "supervisor_recommendation": {"model": "gpt-6.1-sol", "reasoning_effort": "high"},
  "impact_mapper": {"model": "gpt-6.1-sol", "reasoning_effort": "low"},
  "design_research_architecture": {"model": "gpt-6.1-sol", "reasoning_effort": "low"},
  "design_research_validation": {"model": "gpt-6.1-sol", "reasoning_effort": "low"},
  "publication_assignment": {"model": "gpt-6.1-sol", "reasoning_effort": "low"}
}
```

Each omitted entry retains its recommendation. Unknown nodes, malformed selections, inherited
worker models, unsupported harness/model/effort pairs, and cross-provider mismatches are rejected.
Muse is the one explicit cross-provider exception in Codex catalog revision 9. It cannot be the
Supervisor recommendation because the CLI cannot switch the primary thread.
The preview lists `model_options` for deliberate selection. The supported set is separate from
the recommendation matrix, so GPT-6.1 Sol, GPT-6 Sol, and GPT-6 Luna are selectable without changing
the Astra core recommendation. Cursor also offers Gemini, Composer, Sonnet, Opus, and Fable
alternatives. Claude offers Sonnet, Opus, and Fable. Every GPT-5.6 variant is retired for new
selections, allowances, conditional review requests, and native launches, even when a runtime exposes it.
The frozen legacy matrix is not a fallback around retirement. Do not map an effort the
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
Codex revisions 3 through 7 and Cursor revision 3 retain their original recommendations,
model options, bytes, digests, and approvals as historical records.
Revision 6 recommends Astra or GPT-6 Sol at medium effort for helpers and fixed research,
and exposes GPT-6 Luna / max as an economy fanout option. Revision 7 upgrades the explicit
Codex recommendation to GPT-6.1 Sol and adds it as a selectable option in both Codex catalogs.
Revision 7 retains its Astra helper recommendation and Luna economy option.
Revision 8 changes Codex helpers/research/publication/economy fanout to GPT-6.1 Sol low and
removes GPT-5.6 options. Cursor revision 4 removes its GPT-5.6 Sol option without changing
recommendations; Claude revision 3 is unchanged. Revision 9 adds the optional OpenCode Muse
pair and dispatch metadata without changing default recommendations. Revision 8 plans reconstruct
without Muse or the new metadata. Older engines reject unsupported revisions.
Claude revision 4 upgrades recommendations to Sonnet 5.5 and Opus 5.5. Cursor revision 5 adds
both as explicit alternatives while retaining its recommendations. Earlier Claude and Cursor
plans retain their options, assignments, and digests. Fable remains `claude-fable-5-1`.
Opus 5.5 conditional reviewer fan-out supports high/xhigh/max at weights 3/4/5.
Conditional reviewer fan-out also permits GPT-6.1 Sol at high/xhigh/max with the existing
3/4/5 dispatch weights. Its assignments remain separately selected in the runtime-home policy,
not through graph-node overrides. Old policy records and completed fanout requests remain
auditable; new plans and fanout requests reject retired selections. Historical core envelopes
also remain reconstructable because the ledger does not execute model calls. The Supervisor
must refuse new GPT-5.6 launches under any historical plan and obtain a separately approved
current supported plan. Never silently upgrade approved records.

## Speed and cost preference

Request Standard speed for new agents. The native spawn interface has no speed selector, so
inherit configured speed and report `standard unverified/unavailable`. Do not invent `service_tier`
or claim a model choice verifies speed. Fast is not preferred: it consumes 2.5 times included
usage or twice paid credits, which is not a claim about generated-token count.
Fast may be selected when the user explicitly requests it during planning. Record that choice
in the human-approved plan and apply it only through a supported host setting. When native spawn
has no selector, report `fast unverified/unavailable` and inherited speed; do not silently change it.
GPT-6.1 Sol Standard has lower listed credit and input/output prices than GPT-5.6 Sol;
it is not cheaper than GPT-6 Luna. The economy recommendation is the selected policy,
not a verified model-quality or observed-token-saving result. See the official
[Codex pricing](https://developers.openai.com/codex/pricing/) and
[API pricing](https://developers.openai.com/api/docs/pricing).

## Source and evaluation scope

Claude 5.5 IDs and low/medium/high/xhigh/max variants were observed October 5, 2026 in
`opencode models opencode --verbose`. Use `claude-sonnet-5-5` and `claude-opus-5-5` for
the native catalogs. This observation does not establish native Claude or Cursor account access;
verify the exact selected pair on that host before dispatch.

GPT-6.1 Sol guidance checked October 3, 2026:
[OpenAI GPT-6.1 Sol](https://developers.openai.com/api/docs/models/gpt-6.1-sol) and
[Codex and ChatGPT Work models](https://learn.chatgpt.com/docs/models).
Use the exact ID `gpt-6.1-sol`; its API reasoning efforts are low, medium (default), high,
xhigh, and max. The native spawn tool may expose additional efforts; verify that specific
interface before dispatch rather than assuming API and host controls are identical.
OpenAI recommends GPT-6.1 Sol for complex coding when available and describes near-Astra
performance at lower cost. That guidance motivates the Sol upgrade; it is not a local model evaluation.
Availability depends on plan, client, rollout, and workspace settings. Retain supported GPT-6
alternatives for explicit selections; never replace an approved assignment automatically.

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

Historical September pricing guidance recorded lower listed GPT-6 Sol/Luna per-token prices
than their GPT-5.6 predecessors. Artificial Analysis reports stronger
benchmark results for GPT-6 Luna max than xhigh, with greater latency and token use. These are
configured recommendations.
Deterministic tests establish selection, binding, and compatibility, not model quality or live
Claude/Cursor operation. Use the authorized disposable
[behavioral evaluations](behavioral-evaluations.md) before claiming cross-harness execution,
savings, or reliability. Report unavailable telemetry rather than estimating it.
