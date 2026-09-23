# Software Engineering Graph

Canonical source: [ai-skills/skills/software-engineering-graph](https://github.com/JeffreyFerreiras/ai-skills/tree/master/skills/software-engineering-graph).
Maintain the skill here. The former standalone repository provides a public redirect and preserved history.
Run contributor validation from this source skill directory in the ai-skills repository.
Run operational ledger commands from a verified skill copy with `--repo` pointing to the target
repository. Policy lives under that skill copy's `policies/` directory, and ledger state defaults to
its `state/` directory. Absolute `--state-root`, `SOFTWARE_ENGINEERING_GRAPH_STATE_HOME`, and
absolute `XDG_STATE_HOME/software-engineering-graph` remain explicit state overrides. Pass an old root
explicitly to open historical state; the engine does not discover, migrate, or rewrite it.
If overriding the default state root for a multi-command run, set one absolute root before the first
stateful command and keep it for the whole shell session. The stateless usage checkpoint does not
require it:

```powershell
$env:SOFTWARE_ENGINEERING_GRAPH_STATE_HOME = 'C:\graph-state'
```

Software Engineering Graph is an **AI-agent skill** for Codex, Cursor, and equivalent agent hosts. It organizes complex software work
across specialized AI agents and makes scope, human approval, design, implementation, independent
review, and testing explicit.

Use it for non-trivial features, fixes, refactors, migrations, integrations, and
production-sensitive work. Documentation and mechanical changes can use a smaller, faster route.

This repository is authoritative for the AI skill, its local workflow engine, and its nine supported reusable
agent-role definitions. It is not a general-purpose task scheduler, CI service, or security boundary.
The installed profile remains untouched unless separately approved work explicitly changes it.

## What the skill does

The skill keeps the primary host agent in charge as the **Supervisor**. The Supervisor scopes the
request, proposes an execution plan, asks the human to approve that plan, and then coordinates only
the roles the work needs:

- **Impact Mapper** classifies the change and selects the minimum safe route.
- **Architecture and Validation Researchers** perform a fixed, bounded two-member evidence
  fan-out before every Tech Lead design generation. They use the Impact Mapper role, return only
  verified evidence manifests, and have no write, test, decision, or findings authority.
- **Tech Lead** designs the change and plans its implementation.
- **Software Architect** independently reviews the design.
- **Senior Engineer** is the sole implementation writer.
  Every implementation and follow-up repair must apply both `clean-code` and
  `clean-architecture-code`, reporting concrete actions and validation for each. A missing required
  skill blocks READY_FOR_REVIEW; architecture guidance stays within the approved design.
- **Code Reviewer** independently reviews the completed change.
- Every Code Reviewer must apply both `code-review` and `clean-architecture-review`, including
  delegated and follow-up reviews, and report the checks and conclusions from each. Missing either
  skill makes the review incomplete; the Supervisor cannot accept approval without both reports.
- **Test Engineer** verifies the acceptance criteria and regression evidence.
- **Security Reviewer** joins when security, privacy, identity, secrets, or trust boundaries are
  affected.
- **Pull Request Engineer** is the required instruction-level publication role for every repository
  implementation intended for delivery. A fresh host-catalog publication dispatch publishes after the gates
  and may later perform separately approved cleanup. It adds no reusable profile or engine node.

The workflow is deliberately bounded. It limits design and repair loops, separates writing from
review, records evidence, and returns unresolved product or risk decisions to the human.

## How the AI agents work together

1. The Supervisor turns the request into a scoped task brief with acceptance criteria and non-goals.
2. The skill sizes model cost independently from route complexity and proposes exact AI model and
   reasoning-effort assignments.
3. The human approves the plan before any specialist agent starts.
4. Design routes first run the assessed architecture/validation research fan-out. The Supervisor
   seals its evidence collection before creating the same-generation Tech Lead branch, then the
   selected agents continue through bounded design, implementation, review, and test handoffs.
5. After the approved criteria, reviews, and checks pass, the Pull Request Engineer creates exactly one
   review-ready pull request or updates and verifies the exact existing pull request.
6. The Supervisor closes the run only after validating that publication evidence.

When both installed-skill policy and task brief opt in, an approved execution-plan v2 can also contain
conditional review assignments. A primary Code Reviewer may then return a frozen preliminary review
and a typed request using only approved assignment, reason, acceptance, and evidence IDs. The
Supervisor alone dispatches engine-managed graph nodes and conditional reviewer-fanout children and
mutates the ledger. Delegated reviewers receive fresh, read-only envelopes and cannot create another
reviewer-fanout level.

New task/plan v3 proposals include bounded direct helper allowances by default, covered by initial
plan approval. Actual dispatch depends on useful work, budgets, and verified host capabilities;
explicit opt-out and stricter policy are respected. Evidence Scout serves
Tech Lead, Software Architect, Senior Engineer, Code Reviewer, Test Engineer, and Security Reviewer.
Validation Executor serves only Senior Engineer and Test Engineer. Both may directly invoke either
helper within their approved allowance. Helpers retrieve evidence or execute exact selected commands;
parents retain interpretation, decisions, the sole writer, and independent verification.

Read the canonical [economy-helper contract](references/economy-helpers.md) for one-way allowance
hash binding, deterministic register commands, eligibility, host restrictions, budgets, resource
checks, and separate usage accounting. Helpers remain
instruction-level host sessions, never graph branches, reviewer-fanout members, or mandatory gates.
Use direct permitted tools for trivial work. Historical approvals gain no new helper permissions;
repository profiles do not prove that a runtime has loaded or enforced their contracts.

Explicitly requested disposable evaluations may use [cooperative test mode](references/economy-helpers.md#cooperative-test-mode).
An approved schema-2 allowance binds a temporary fixture and acknowledges missing isolation. Only
confinement checks are waived; all other helper checks stay active. Responses are marked
`cooperative_test` and `production_evidence: false`. Normal runs keep strict requirements.

A local control ledger tracks assignments, approvals, retries, active-work ownership, and recovery
so the workflow behaves consistently and deterministically. It coordinates agents but does not
execute them. The Supervisor is the sole ledger operator and remains the user-facing decision maker.
It performs bounded read-only repository preflight and owns creation of the initial implementation
worktree and branch with `git worktree add` within existing implementation authority. The worktree is
a standard Git worktree, not a Codex-managed worktree. This sole setup exception does not
authorize modifying existing worktrees. It validates publication evidence but never commits, pushes,
creates a pull request, or removes a worktree. The Senior Engineer remains the sole source and test
writer and never publishes.

The Supervisor presents an adjustable execution plan before starting branches. Codex catalog
revision 4 recommends these assignments; Claude and Cursor remain on revision 3. They are defaults,
not required models:

| Catalog | Helpers / research | Core implementation / Tech Lead | Architecture / review |
| --- | --- | --- | --- |
| `codex-astra` | GPT-6 Luna `low` | GPT-6 Astra `medium` | GPT-6 Astra `high` |
| `codex` | GPT-6 Luna `low` | GPT-6 Sol `medium` | GPT-6 Sol `high` |
| `claude` | Sonnet 5 `low` | Opus 5 `medium` | Opus 5 `high` |
| `cursor` | Gemini 3.8 Flash `low` | Grok 4.7 `medium` | Grok 4.7 `high` |

The Astra catalog also permits GPT-6 Sol and GPT-5.6 Terra selections. Other supported
alternatives appear in the preview's `model_options`. Verify actual host/account support before approval and dispatch.
Catalog knowledge does not prove runtime availability. No automatic fallback changes an approved
assignment. An unavailable recommendation should prompt a proposed alternative, not a default-only block.

Run `graphctl --repo <repo> policy` to load or create the policy beside the installed skill. Then run
`graphctl plan --run-id <id> --task-brief <path> --host <catalog>`
to preview without initializing ledger state. Adjust `model_overrides` in task-brief v2/v3, preview
again, then `init` and approve the exact plan digest. After initialization the brief is immutable,
including while approval is pending; changed choices require a new run, not edits to stored JSON.
The returned policy digest binds the task brief and plan.

The plan lists assignments and dispatch conditions, Supervisor/publication choices, model options,
and a helper recommendation. The Supervisor explains the actual work sequence, conditional roles,
cost/quality tradeoffs and uncertainty, and lets the human choose. The actual primary model is not
switched by the CLI. Missing trusted Supervisor model/effort metadata retains advisory mode:

> Supervisor warning: This Supervisor is an advisory role and thought partner. Treat its plans, decisions, and synthesis as recommendations requiring your approval.

Advisory mode adds no approval gate. Routine work uses the existing approval; material assignment
changes require a new plan. Historical unversioned and prior explicit catalog revisions keep exact
assignments and digests. Codex role TOMLs are installable defaults, not requirements on Claude,
Cursor, or a differently approved Codex plan. Installed copies require separately authorized sync.
See [model catalogs](references/model-catalogs.md) for selections, compatibility, and limitations.

The five executable routes are `advisory` (read-only review), `design_only` (research and independent
design approval), `fast_path` (mechanical/documentation implementation plus independent review and
testing), `delivery_only` (implementation-ready work plus independent review and testing), and
`full_delivery` (research, design, implementation, review, and testing). `delivery_only` requires a
v2 or v3 task brief with complete acceptance criteria, resolved implementation and architecture
decisions, no unresolved human decisions, no high-risk sizing inputs, and no security/privacy tag.
The Impact Mapper may escalate it to `full_delivery` when repository evidence contradicts readiness.
Legacy valid repository policies with the prior four-route topology gain this engine-owned route in
memory, so adopting the updated skill does not require editing the consumer repository configuration.

Task-brief schema v2 makes model sizing explicit with `scope_extent` and `uncertainty`. Bounded,
low-risk, low-uncertainty work with no mandatory impact tag selects `small`, even when the approved
route is `full_delivery`. Medium risk, cross-file scope, medium uncertainty, or a non-security
mandatory tag selects `medium`. High or critical risk, `security_privacy`, high uncertainty, or
broadly cross-cutting scope selects `large`; high risk mapping to large is intentional. Explicit v2
overrides may raise cost but cannot go below the computed safety floor.

The route still determines workflow gates. A v2 small `full_delivery` run keeps the same research,
design, implementation, independent review, testing, specialist, consolidation, and closure topology
as medium or large. Revision 3 recommends the same baseline models across sizes; the human can adjust
them to the task before approval. Existing task-brief v1 inputs retain the
legacy route-influenced classifier, unrestricted explicit override, execution-plan shape, and digest.

Repository-policy schema v2 permits project-specific bounded repository file/directory references,
exact required-check command IDs, and an `implementation_roots` classification. Senior Engineer
writes must fit a classified implementation or artifact root; other permitted writes must fit an
artifact root. Implementation and artifact roots cannot equal, contain, or be contained by each
other under the host platform's path-case rules; a trailing slash does not create a distinct
location. It preserves role effect/action and external-target ceilings, link/secret protections,
mandatory gates, checks, and sole-writer rules. Policy v1 rejects the new field and retains its
frozen paths, command set, validation behavior, envelopes, and digests.

Task/plan v3 applies only when a run opts into the deterministic helper register. The plan binds one
allowance reference/hash. The run-local register key binds state-root, repository, and run identity;
its immutable context binds the resulting plan and allowance hashes. Replacing either input within
the same run conflicts rather than creating fresh budgets. Graph initialization also recomputes each
parent's effective task/policy/role capabilities and rejects a broader allowance. Task/plan v1 and
v2 bytes remain unchanged and grant zero authority through the new register. Historical
instruction-level helper contracts are not rewritten or revoked.

## Using the skill

In a Codex or Cursor environment where this skill is installed, ask the host agent to use
`software-engineering-graph` for the task. For example:

> Use the software-engineering-graph skill for this feature. Before any specialist agents start,
> show me the exact AI-agent roles, models, and reasoning-effort levels you propose, and ask me to
> approve the plan.

The installed skill is the policy and ledger home. For each repository, `graphctl policy` creates a
bounded policy under the skill's `policies/` directory when none exists. The default ledger lives in
its `state/` directory. A repository-local `.codex/engineering-graph.json` is ignored. The generated
policy covers up to 128 ordinary top-level files and directories, excludes sensitive and agent-instruction
paths, and requires `git diff --check`; inspect and adjust its roots and project checks before plan
preview. A malformed installed-skill policy fails closed. No policy is written into the consumer
repository, and creation does not itself authorize implementation, publication, or external effects.

## Observed token usage

`status` includes optional `usage` accounting from explicitly associated Codex session JSONL files.
It reports **input + output tokens**, with cached input and reasoning output as subsets, and
cache writes as a separate metric. It never adds these subsets to the headline total, estimates
prices, substitutes planned assignments for observed models, or changes a delivery gate.

Before scoping starts, obtain a sanitized checkpoint for the primary session. This read-only
command requires neither `--repo`, repository policy, nor an initialized ledger:

```text
python scripts/graphctl.py usage checkpoint --session-log <explicit-session-file>
```

Keep the returned `source_id`, `offset`, and `prefix_sha256`. After initialization, bind that
checkpoint to the primary scoping interval:

```text
python scripts/graphctl.py --repo <repo> record usage --run-id <run> --action bind --session-log <file> --phase scoping --generation 0 --start-offset <offset> --source-id <digest> --prefix-sha256 <digest> --op-id <id>
```

The three historical checkpoint options are all-or-none. The engine verifies the exact source
identity, byte prefix, and a validated cumulative snapshot boundary (or offset zero). A mismatch
rejects the mutation without silently taking a new baseline. Omitting all three starts at the
latest validated snapshot and excludes earlier history; previously executed phases remain unavailable.
An offset-zero interval includes the first cumulative total only when validated last-usage counters
equal that total. Otherwise the first total becomes a baseline and coverage reports a prefix gap.

Associate a branch session with `--branch-id <id> --attempt-id <id>` instead of `--phase` and
`--generation`. Role, phase, and generation are derived from that executed attempt. Retries have
distinct attempt IDs; resumed sessions can bind to the same attempt. Delegated reviewers bind their
own session and attempt. Both reusable helpers are instruction-level sessions, not ledger branches;
their lifecycle and usage are registered separately by the parent, and parent totals never include
child rollups. Do not associate overlapping work with multiple runs: the engine checks intervals within
a run and does not inspect other runs.

```text
python scripts/graphctl.py --repo <repo> record usage --run-id <run> --action collect --binding-id <binding> --session-log <file> --op-id <id>
python scripts/graphctl.py --repo <repo> record usage --run-id <run> --action close --binding-id <binding> --session-log <file> --op-id <id>
python scripts/graphctl.py --repo <repo> status --run-id <run> --json
```

Close the current primary phase, then bind the next phase using the returned checkpoint. The five
phase names are `scoping`, `research_design`, `implementation`, `review_testing`, and `closure`.
Use fresh operation IDs for new samples; repeating the same normalized request and operation ID
replays its original result, even after an append. Changing explicit input under an existing ID
conflicts. Collections of already consumed snapshots never add tokens again.

Reports include cumulative totals, all five phases, roles, branch agents, attempts, generations,
models, efforts, and model/effort pairs. `observed_totals` retains known usage; `complete_totals`
is null when coverage is incomplete. `coverage` is `unavailable`, `partial`, or `complete`;
optional missing/discontinuous metrics remain null and appear in `metric_partial`. An unexecuted
phase or skipped role has no measured consumption. `missing_executed_attempts`,
`missing_primary_phases`, and `association_coverage` separately expose association gaps.
Role, agent, and generation summaries retain their relevant missing associations and cannot call
a measured first attempt complete while an executed retry remains unbound. Supervisor summaries
also retain missing primary phases by generation. Model, effort, and model/effort summaries state
`attribution_scope: observed_intervals_only` and expose run `association_coverage`; they do not
assign an unbound attempt to its planned model or effort.
Active runs remain provisional (`running: true`, incomplete aggregate coverage); open intervals,
late telemetry, resets, malformed records, and unknown contexts remain visible. Completion means
only the declared, closed, observed intervals are covered, not an attestation of all host usage.
The final response can itself consume tokens beyond the last checkpoint.

At phase boundaries, settled increments can be attributed even within a turn. A response that
spans a checkpoint, or multiple model/effort contexts between snapshots, can make attribution
ambiguous. The engine counts such in-run increments once as unattributed and marks coverage
partial. It excludes increments that could contain pre-run work, and never guesses by elapsed time.
Counter resets preserve prior observations and omit the uncertain bridge. Truncation, replacement,
identity change, or prefix rewriting cannot silently restart a binding; use an explicit new binding
for a replacement and retain the disclosed gap.
For a replacement retaining the same session ID, earlier overlapping offsets remain rejected.
Recovery requires a new source identity or a later nonoverlapping checkpoint; rebinding cannot
bypass the existing interval deduplication rule.

The versioned `codex_jsonl_v1` adapter recognizes `session_meta.payload.id`, observed
`turn_context.payload.model` and `effort` (or `reasoning_effort`), and
`event_msg` / `token_count` / `info.total_token_usage` cumulative counters. Required input, output,
and total counters are nonnegative signed 64-bit integers, with total equal to input plus output;
booleans are invalid. Optional fields are `cached_input_tokens`, `reasoning_output_tokens`, and
the observed `cache_write_input_tokens`, normalized in reports to `cache_write_tokens`.
The legacy source alias `cache_write_tokens` is accepted only when `cache_write_input_tokens`
is absent. The observed spelling wins even if its value is invalid, which yields a null metric
rather than falling back to the alias. Aggregates use exact arbitrary-precision integers so valid
intervals across resets or sessions can exceed the raw signed 64-bit counter limit without
overflow; consumers must preserve JSON integer precision. Raw source bounds remain unchanged.
Only the finite host model catalog and finite effort values survive parsing;
other observed names become `unknown`. Other formats or missing metadata report partial or
unavailable usage. This observed-log-format adapter is not a claim of a stable public Codex API.

Only an explicit regular file is read, with symlinks/reparse points and identity changes rejected.
Where supported, the file is opened nonblocking before verifying its regular-file type again,
so replacement by a FIFO between the initial check and open cannot block the reader.
Each invocation is bounded to 64 MiB, 1 MiB reads/records, 100,000 records, and JSON depth 24.
Incomplete trailing lines wait for a later collection. No session search, directory crawl, raw
conversation, session ID, source path, credentials, or arbitrary source strings enter usage events,
operation responses, or diagnostic output. The ledger stores hashed source identity, byte-prefix
checkpoints, normalized counters/context, and fixed diagnostics. These hashes prove continuity,
not authenticity; association remains the caller's responsibility. Existing schema-6 runs require
no migration and report unavailable usage until metadata is associated. Optional usage corruption
does not disable ordinary graph validation or delivery. Usage collection is allowed after complete
or aborted runs, solely to settle late accounting metadata.

## Repository map

- [`SKILL.md`](SKILL.md) defines the AI skill and its operating contract.
- [`profile-agents/`](profile-agents/) contains the nine supported reusable Codex role profiles. Cursor runs
  keep those files unchanged and resolve models through the host catalog instead.
- [`graph_engine/`](graph_engine/) implements deterministic planning, validation, and local state.
  Host catalogs in `graph_engine/hosts.py` map role intelligence classes onto Codex or Cursor models;
  `helper_register.py` owns the separate host-only helper reservation record.
- [`scripts/graphctl.py`](scripts/graphctl.py) is the command-line adapter used by the Supervisor.
- [`scripts/helper-register.py`](scripts/helper-register.py) initializes, preflights, reserves, settles,
  and reads the optional task/plan v3 helper register.
- [`references/`](references/) contains schemas and workflow contracts.
- [`docs/technical-design.md`](docs/technical-design.md) explains the internal architecture and
  compatibility guarantees.
- [`tests/`](tests/) contains the standalone acceptance and behavior tests.

## Requirements and boundaries

- Python 3.9 or newer
- Python standard library only
- Local operation only, with no CI or remote automation added by this repository
- Pull-request publication is a required instruction-level delivery contract for repository
  implementation, not an engine-enforced topology or remote provider implementation
- Engine 2.6.0 initializes semantic format 6. Explicit `evidence enable --contract-version 2`
  activates format 7 using the same tables. Historical plans, approvals and policies are unchanged;
  old engines refuse enabled runs. There is no downgrade. Schema-5 runs still require their old engine.
- Default ledger state and policy live beside the installed skill; historical state roots must be
  passed explicitly
- Repository-policy v1 and task/plan v1-v2 retain their historical behavior and bytes; policy v2
  adds bounded project paths/checks, and task/plan v3 opts into the separate helper register
- Revision 3 assignments are adjustable recommendations; the chosen supported model/effort is
  bound to approval. Historical plans preserve their model-class constraints and recorded assignments. Research output
  contracts require an `evidence_manifest`, verified evidence, a null decision, and empty findings.

Implementation authorization and initial plan approval cover the plan's exact non-force commit, push,
and PR actions after all gates; no later publication approval is needed. The Supervisor and Senior
Engineer never publish. Successful delivery requires one review-ready PR, or the exact existing PR
updated and verified; draft only on explicit request.

Publication uses the dedicated implementation worktree, exact repository/remote/base/head, and reviewed
commit or exact staged-plus-unstaged diff. It rejects other tracked, untracked, unapproved ignored, conflicted, or
Git-operation state, identity mismatch, secrets, ambiguity, duplicates, force, amend, or history rewrite.
Only verified generated files within exact plan-approved artifact directories may remain ignored
during publication; they must be absent from the index and reviewed diff, contain no secrets or
unrelated content, and never be force-added or deleted to bypass checks. Cleanup still refuses all
ignored entries. Read the full [publication contract](references/publication.md) before planning or publishing.
Passing local gates triggers the Pull Request Engineer's commit, push, and PR handoff. Successful
repository implementation ends with a verified PR URL; a publication blocker means incomplete delivery.

Before publication or cleanup, the Pull Request Engineer selects and fully reads the smallest relevant
set from its exposed catalog and repository-declared local skills, without crawling other skill trees or
naming an optional skill. Skills cannot expand authority or effects; controlling instructions win, and
conflicts or unavailable content are reported. Each handoff reports `Skill usage`, including provenance,
relevance, failures, or `None`.

After required PR approval and separate cleanup approval, a fresh approved publication-model dispatch may run from any
safe checkout or execution context outside the exact clean, registered target; no cleanup worktree is
created. It uses only non-forced `git worktree remove`, preserves the branch, and refuses dirty,
untracked, ignored, locked, or ambiguous state, recursive deletion, force, prune, or branch deletion.

On Windows, the ledger requires `--ack-degraded-permissions` because Python cannot prove exclusive
profile permissions. `--ack-degraded-durability` is only for environments where directory syncing is
unavailable and that limitation is acceptable. These flags acknowledge platform limitations; they
do not grant extra authority.

## Contributor validation

The [behavioral evaluation protocol](references/behavioral-evaluations.md) supplies reproducible
host scenarios for approval, routing, artifacts, steering, and model selection. These are live-agent
evaluations, separate from deterministic engine tests; no live Astra results are claimed here.

Run only the focused acceptance suite below, with bytecode disabled:

```powershell
$env:PYTHONDONTWRITEBYTECODE = '1'
python -m unittest -v tests.test_contracts tests.test_planner tests.test_validator tests.test_state tests.test_cli tests.test_graph_hardening tests.test_reviewer_delegation
```

After final review, run the local read-only hygiene check separately and last:

```powershell
$env:PYTHONDONTWRITEBYTECODE = '1'
python -m unittest -v tests.test_standalone_acceptance.StandaloneAcceptanceTests.test_hygiene
```

The hygiene check verifies forbidden artifacts, required ignore patterns, the exact supported nine-profile inventory,
repository authority wording, skill-discovery guardrails, and stale external requirements. It does
not repair files or inspect an installed profile or consumer repository.
