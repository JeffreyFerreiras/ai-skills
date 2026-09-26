---
name: software-engineering-graph
description: Orchestrate software delivery with approved role/model plans, bounded loops, and independent review and testing. Use for requested graph engineering, multi-agent delivery, or repository-mandated orchestration.
---

# Software Engineering Graph

Use the local control ledger for every new graph run. Treat it as a deterministic coordination and
recovery aid, not a security boundary or a model-agent executor. Keep the primary agent as Supervisor
and the sole `graphctl` and ledger CLI mutator. The Supervisor alone dispatches engine-managed graph
nodes and conditional reviewer-fanout children. Eligible role parents may spawn bounded reusable
helpers under the instruction-level contract below; those children never become ledger
branches or reviewer-fanout members. The execution-plan-authorized Pull Request Engineer is the sole
bounded Git, GitHub, and worktree mutator for publication and cleanup and never operates the ledger.
Never give branch agents database paths or operation IDs.
The first Supervisor step is an execution-plan preflight: T-shirt size the job as small, medium, or
large, select only pertinent roles, assign each possible role a model and reasoning effort, and explain
the size, route floor, assignments, and omitted roles to the human. No branch may execute until the
human explicitly approves that immutable execution plan.

Repository policy is not an input to this workflow. Before preparing a task brief, run
`graphctl --repo <repo> policy`. The engine loads a per-repository policy from `policies/` in the
selected runtime home, creating a conservative schema-3 policy there if absent. By default the
runtime home is the sibling `<profile>.local/software-engineering-graph/` directory (for example,
`~/.agents.local/software-engineering-graph/`). An absolute `SOFTWARE_ENGINEERING_GRAPH_HOME`
overrides it; use the same value for the whole run. The CLI returns the policy path, runtime home,
lessons path, artifact root, and policy digest for the task brief. Do not ask
the human to create tool-specific files. Do not write configuration into the consumer repository.
Inspect the generated policy's implementation roots and required checks; update that runtime-home
policy before plan preview when the task needs other paths or project-native checks. The generated
`git diff --check` is a baseline and does not replace relevant tests or builds. A present but invalid
or incompatible policy fails closed. A repository-local
`.codex/engineering-graph.json` is ignored. Missing policy never grants write, publication,
deployment, helper, or external-system authority by itself. After the policy is loaded, follow the
normal plan, approval, and ledger workflow.

For schema-3 policies, put each new run's task brief, helper allowance, design, review, and evidence
files under `<runtime-home>/artifacts/<repository-id>/<run-id>/`. Use `runtime:artifacts/<repository-id>/<run-id>/<file>#sha256=<digest>`
for content references and unhashed `runtime:` paths where the contract requests them. Put control
manifests in the run's `graph-inbox/` directory. Existing repository source can still be cited with
`repo:` references, and project deliverables still belong in the consumer repository. Historical
schema-1/2 policies retain their repository artifact roots. Never put new run assets in the installed
skill or consumer repository when using schema 3.

At the start of that preflight, before substantive scoping work, take a read-only
`usage checkpoint --session-log <explicit primary session file>` when Codex token metadata is
available. It runs before runtime-home policy or ledger initialization. Retain only its sanitized
checkpoint fields and bind them to `scoping` after initialization; never count unrelated primary
thread history. If metadata is unavailable, report token usage as unavailable rather than omitting
it or inferring consumption from the plan. Read the token-accounting procedure in
[Ledger operations](references/ledger-operations.md) before collecting metadata.

After the usage checkpoint and before scoping each new run, read `lessons-learned.md` in the
selected runtime home. If it does not exist, copy the tracked
[starter document](assets/lessons-learned.md) there. Apply relevant lessons as context, then verify
them against the current request, repository, and approved plan; the journal never overrides those
authorities or creates a new gate. During the run or at closure, record concrete, verified lessons
that would change a future Supervisor decision. Update an existing lesson when new evidence corrects
it, and record the run and evidence reference without secrets or raw session content. Keep this
write in the selected runtime home, never in the consumer repository or tracked skill source. If the
journal cannot be read or updated, report the gap and continue the authorized workflow.

For new repository implementation work, the Supervisor first performs bounded read-only inspection
of repository instructions, status, worktree registrations, and the intended base. Within existing
implementation authority, the Supervisor owns setup: create one new isolated implementation worktree
and branch with `git worktree add` before editing project files. Use a standard Git worktree, not a
Codex-managed worktree created through `create_worktree`. This is its sole Git/worktree mutation exception; it does
not authorize commits, publication, cleanup, force, or changes to an existing checkout. Resolve the
exact target path and base first, and preserve unrelated work. If the selected policy forbids setup,
report that concrete constraint and continue authorized read-only preparation.
The task brief records the selected worktree and branch as scope context. Reuse an existing checkout
or worktree when the user explicitly directs it; still inspect its status before delegating.

Name the host catalog in the execution plan as `claude`, `codex`, `codex-astra`, or `cursor`.
`codex-astra` is the default model catalog for the Codex runtime. Do not infer the host from a
task, prompt, environment variable, or agent self-report. Use a trusted host runtime assertion, or
ask the human. Pass `--host claude` to `init` when running in Claude Code and `--host cursor` when
running in Cursor; omit it or pass `--host codex-astra`
for the default Codex catalog. Use `--host codex` to recommend Sol for core roles instead of Astra.
Verify that the host supports every planned model and effort. Changing catalog is a new plan.
Before choosing or dispatching a catalog, read [Model catalogs](references/model-catalogs.md).

Present model assignments as recommendations, not prerequisites. New Codex catalog revision 6
suggests GPT-6 Astra `medium` for helpers, fixed research, and core work, and `high` for review;
the explicit `codex` catalog suggests GPT-6 Sol instead of Astra. Offer GPT-6 Luna `max` as the
economy fanout option in the preview. Select it through approved graph-node overrides for fixed
roles or the separately bound allowance for direct helpers. GPT-6 Sol and GPT-5.6 Terra
are also selectable within the Astra catalog. Claude suggests Sonnet 5 `low` for helpers,
Opus 5 `medium` for core work and `high` for review. Cursor suggests Gemini 3.8 Flash `low`
for helpers, Grok 4.7 `medium` for core work and `high` for review. These are human-selected
starting preferences, not benchmark claims.

Before initialization, present the execution sequence, relevant and conditional roles, model/effort
for each role, the bounded helper allowance, available alternatives, and known availability gaps.
Invite the human to adjust assignments. Use task-brief v2/v3 `model_overrides` for graph nodes,
Supervisor recommendation, and publication assignment. Helper selections live in the separately
bound allowance. `graphctl plan` previews the candidate without writing state using the selected
policy.
Repeat the preview after requested changes, then initialize the final brief and approve its exact
plan digest. Do not edit a brief after initialization, even while approval is pending. Such changes
require a new run and approval; never reset consumed budgets for equivalent retries.
A preferred model being unavailable is a reason to propose an available alternative, not insist on
the default. Verify the selected pair before dispatch. Never silently substitute a different pair.

Missing catalog revisions, prior explicit revisions, assignments, and approvals reconstruct unchanged.
New Codex revision 6 plans cannot be consumed by older engines. Role TOML pins are Codex installation
defaults only, not cross-harness requirements. Dispatch a supported fresh equivalent-contract agent
when a named profile does not match the approved plan. Models listed as options are not proof of
runtime availability or live cross-harness delegation.
Report the actual Supervisor model and effort only when a trusted host runtime assertion makes both
values verifiable. If either value is missing, unverifiable, or different, operate in advisory mode
and display this exact warning once per run, repeating only if verification status changes:

> Supervisor warning: This Supervisor is an advisory role and thought partner. Treat its plans, decisions, and synthesis as recommendations requiring your approval.

Do not infer verification from a task, prompt, environment variable, or agent self-report. Current
local operation without a trusted host assertion is advisory.
Advisory mode describes model verification, not an additional approval gate. Once the execution
plan is approved, continue routine in-scope decisions, checks, and bounded retries under that approval.
Ask only for a required human decision or a material change to approved scope, authority, or assignments.
If a controlling instruction causes a pause, cite its exact file and instruction and explain the
concrete conflict. Do not infer new approval requirements from optional skill guidance.

## Ledger procedures

For explicitly enabled format 7 runs, use the repairable evidence and planning-constraint procedures
in [Ledger operations](references/ledger-operations.md). Produce, check, inspect the cause, correct
within authority, and retry within the existing allowance. Return repairs to their originating gates;
fresh checks never refresh an older review's source binding. Keep all mandatory independent gates.

Before any ledger operation, preserve these invariants: the Supervisor is the sole ledger mutator;
the immutable execution plan requires explicit human approval before a claim; every attempt-scoped
mutation presents the current claim fence; budgets never reset through retries or relabeling; branch
agents receive only their claimed envelope; resume settles every running attempt from evidence or an
explicit fenced timeout; and platform degradation acknowledgments grant no authority.

Then read the operation-relevant section of [Ledger operations](references/ledger-operations.md):
`Start a run` for initialization, approval, research fan-out, and platform acknowledgments;
`Instruction-level helpers` or `Optional reviewer delegation` before those paths; `Operate the
ledger` for claim, record, join, retry, resume, abort, or completion; and `Token accounting at phase
handoffs` for usage commands. The first branch is always the Impact Mapper. Direct evidence children
use the separate contract below and never receive ledger control metadata.

## Operating model

Treat a new user message as steering the active task unless it clearly cancels or replaces it.
Answer status questions briefly, then continue. Incorporate routine clarifications without restarting
the run. If a change invalidates immutable scope or acceptance criteria, stop dependent dispatches,
settle running attempts through the ledger, and prepare a new brief and plan; retain valid evidence
and report what must be revalidated. Never silently edit approved artifacts or discard completed work.

Use concise user updates: outcome or current blocker first, then the next action. Keep detailed
protocol packets in artifacts. Before evaluating or changing prompting or catalog defaults, read
[Behavioral evaluations](references/behavioral-evaluations.md); engine tests alone do not establish
agent behavior.

At each major phase handoff and in the final response, report observed input/output/total tokens,
cumulative run usage, and coverage, with role/agent and observed model/effort comparisons when
available. Use the five accounting phases `scoping`, `research_design`, `implementation`,
`review_testing`, and `closure`. Close the primary interval and bind the next phase at the returned
checkpoint; associate each executed branch attempt and each permitted helper session
separately, without folding child usage into its parent.
Report gaps, unavailable phases, and provisional running usage explicitly. Never present a skipped
role as measured consumption or a partial total as complete. The final response itself may add
tokens beyond its last checkpoint. Accounting is additive metadata and must not introduce a new
approval, topology, or delivery gate.

Treat the primary agent as the Supervisor. Keep requirements, decisions, approvals, and user communication in the primary thread. Dispatch the roles required by the selected executable route and synthesize their results. Use the route mapping below; do not remove a mandatory gate to reduce model cost.

Follow applicable repository instructions before this workflow. Let the repository define architecture, risk triggers, commands, specialists, and completion gates. Do not let this skill expand the user's requested scope or authority.

### Bounded role skill preflight

Before the Senior Engineer or Code Reviewer takes task actions, require that role to inspect only the
skill catalog exposed to its current session and local skills explicitly declared by applicable
repository instructions. Do not crawl arbitrary profile or global skill directories. Select the
smallest clearly relevant skill set for the assigned implementation or review task, then read every
selected `SKILL.md` fully before acting. Do not prescribe a specific optional skill by name.

Required implementation skills: `clean-code` and `clean-architecture-code`. Every Senior Engineer
must read both SKILL.md files fully before acting and apply both workflows, including delegated
implementation and follow-up repairs. Include this requirement in every Senior Engineer dispatch
and continuation. These skills are mandatory; optional skill selection is additional.

Required review skills: `clean-code-review` and `clean-architecture-review`. Every Code Reviewer must
read both SKILL.md files fully before acting and apply both workflows, including delegated and
follow-up reviews. These two skills are mandatory, not optional selections based on apparent
architectural impact. Include this requirement in every Code Reviewer dispatch and continuation.

Discovered skills may change the role's method only. They must not expand the user-approved scope,
role authority, model or reasoning effort, writable files, allowed tests or commands, delegation,
external effects, or permission to install, synchronize, remove, or mutate skills, profiles, or
consumer repositories. User instructions, repository instructions, approved task artifacts, and the
role profile control any conflict. Decline a conflicting skill instruction and report the conflict in
the role's risks or observations.

If the catalog is unavailable or a selected skill cannot be read, report the condition without
inventing skill content. A Code Reviewer missing either required skill must report an incomplete
review; do not accept APPROVE or substitute a generic review. A Senior Engineer missing either
required implementation skill must return DESIGN-BLOCKER, not READY_FOR_REVIEW. Optional skill
failures may proceed only when controlling instructions remain sufficient and both required
workflows for the assigned role can still be completed.
Senior Engineer and Code Reviewer handoffs must each include a `Skill usage` section listing every
selected skill's name, safe source or provenance, and relevance reason. `None` is not permitted for
a successful implementation or review handoff. A completed Senior Engineer handoff must report
concrete implementation actions and validation for each required skill. The Supervisor must reject
a READY_FOR_REVIEW handoff missing either report. A completed Code Reviewer handoff must report
concrete checks performed and conclusions for each required skill. The Supervisor must reject a
successful review handoff missing either report. If no architecture change is present, report that
evidence-backed assessment without expanding scope; neither workflow may be silently skipped.

Reviewers identify risk; they do not own scope. The Tech Lead must challenge a requested revision
that is not traceable to the immutable task brief. The Supervisor is the binding scope authority and
must resolve scope before a finding can consume a revision round.

Use these base roles when available:

- `tech_lead`: author the technical design and implementation plan.
- `software_architect`: independently approve or reject the design.
- `senior_engineer`: act as the sole implementation writer.
- `code_reviewer`: review the completed diff without editing it.
- `test_engineer`: independently verify behavior and acceptance criteria.
- `security_reviewer`: join only when security, privacy, identity, secrets, or trust boundaries are affected.

Use repository-defined specialists when its routing rules require them. If a named profile is unavailable,
the Supervisor may spawn a bounded agent with the same contract instead of weakening a required gate.
That graph-role fallback does not grant helper authority; follow the separate approved allowance.

The Impact Mapper selects route, risk, and specialist tags only. The Supervisor owns fan-out
eligibility after checking branch dependencies and shared resources.

### Direct evidence children and validation helpers

Read [Economy helpers](references/economy-helpers.md) before granting an allowance or spawning
a helper. It is the canonical parent eligibility, approval, discovery, resource, lifecycle, context,
and usage contract. Child behavior lives in the referenced reusable helper profiles.

The Evidence Scout formalizes the existing direct evidence-child mechanism. Tech Lead, Software
Architect, Senior Engineer, Code Reviewer, Test Engineer, and Security Reviewer may invoke it.
Only Senior Engineer and Test Engineer may invoke Validation Executor for exact parent-selected
commands. Both parents may directly spawn either helper within an approved allowance, without
per-call Supervisor dispatch or human approval. Neither helper writes source/tests or makes role
decisions. No arbitrary agents, graph nodes, reviewer-fanout children, or grandchildren are allowed.

Enable bounded helper fan-out by default when preparing new execution plans for eligible roles.
Include the allowance in the initial plan approval; do not require a separate opt-in or per-call
approval. Parents should delegate useful independent evidence work and permitted validation within
that allowance. Actual dispatch remains conditional on available work, budgets, resource safety,
and verified host capabilities; use direct tools for trivial work. Honor explicit user opt-out and
stricter selected policy, and record any omitted or unsupported allowance with its reason.

These are host-only sessions, not engine branches or new gates. New deterministic
registration requires a human-approved task/plan v3 allowance attachment and the separate run-bound
helper register. Task/plan v1 and v2 have no authority through that register; loading new source does
not rewrite or revoke a historical run's separately approved instruction-level contract. Prefer
direct permitted tools when delegation overhead exceeds the likely benefit.
Profile availability and prompt restrictions do not prove host enforcement; verify the actual
assignment and capabilities before dispatch.

For explicitly requested disposable evaluations, the [helper test mode](references/economy-helpers.md#cooperative-test-mode)
can waive unavailable confinement checks without pretending they passed. It requires a new approved
allowance bound to that temporary repository. It is never an automatic production fallback; model,
scope, command, role, and budget checks still apply, and its results are not production evidence.

## Delegation transparency

<!-- dispatch-transparency:start -->
Immediately before every dispatch, tell the user the concrete agent or task name, the bounded scope,
the exact approved model, and the exact approved reasoning effort. This applies to every initial dispatch,
fan-out member, retry, replacement, follow-up, and same-role continuation. Refuse the dispatch when
the concrete identity or any approved assignment value is unavailable, unverifiable, or mismatched;
do not infer, substitute, or silently inherit missing values. When dispatching several agents together,
use one compact announcement that lists every concrete name and identifies which work will run in parallel.
<!-- dispatch-transparency:end -->

Eligible helper parents apply this transparency requirement themselves: announce the
child identity, bounded scope, exact approved model, and exact approved reasoning effort, then register
its lifecycle. This does not require a separate Supervisor dispatch roundtrip.

Resolve model and effort from the approved execution plan. The plan names the host catalog, then uses
the role intelligence-class matrix with that catalog's vendor mapping and revision overrides.
Codex role profiles, including helpers, match Astra revision 6 recommendations. A Luna `max`
economy selection may require a supported fresh agent with the same role contract.
These pins do not override the human's selected model/effort. If a value is not exposed, report it
as unavailable and propose a supported alternative for approval before dispatching that role.
Use the plan's `dispatch_model` and exact approved effort with the host's native dispatch interface;
do not guess an effort-specific model alias. A retry, replacement, or follow-up host, model, or
effort change requires a new plan and approval.

## Select the route

T-shirt size the job before assigning intelligence. Task-brief v2 uses its structured `model_sizing`
inputs: small requires bounded scope, low risk, low uncertainty, and no mandatory impact tag; medium
covers medium risk, cross-file scope, medium uncertainty, or a non-security mandatory tag; large is
required for high or critical risk, `security_privacy`, high uncertainty, or broadly cross-cutting
scope. High risk intentionally maps to large. A v2 explicit override below that safety floor is refused.
Task-brief v1 retains the legacy classifier and override behavior.

Size is a model-cost tier, not a proxy for route selection. In particular, v2 `full_delivery` preserves
every design, implementation, review, testing, and specialist gate while bounded low-risk work may use
small economy assignments. A large task may still use only the roles pertinent to its approved scope.

Select one of the five executable routes. Every route begins with the Impact Mapper and retains
its engine-defined joins and Supervisor consolidation. This mapping is authoritative:

- `advisory`: answer, diagnosis, or review only; a read-only advisory reviewer, then closure. No implementation.
- `design_only`: research pair, Tech Lead, Architect and required design specialists, then design closure.
- `fast_path`: documentation or clearly mechanical changes that cannot affect production behavior,
  dependencies, data, security, operations, or user experience; Senior Engineer, Code Reviewer,
  Test Engineer and required delivery specialists. Initial research and design gates are omitted.
- `delivery_only`: implementation-ready work whose immutable v2 or v3 task brief identifies the
  authoritative requirements source and asserts complete requirements and acceptance criteria,
  resolved implementation and architecture decisions, and no unresolved items. Risk must be low or
  medium; scope must be bounded or cross-file; uncertainty must be low or medium; security/privacy
  impact and unresolved human decisions are forbidden. Senior Engineer, Code Reviewer, Test Engineer,
  and required delivery specialists run; initial research and design gates are omitted.
- `full_delivery`: implementation with unresolved design work or ineligible delivery-only risk;
  research
  pair, Tech Lead, Architect and required design specialists, Senior Engineer, Code Reviewer,
  Test Engineer and required delivery specialists.

Record the selected route and why it applies; omit roles only as specified by that route. A detailed
Jira is sufficient for `delivery_only` only when the frozen task brief carries every required readiness
assertion. The Impact Mapper must escalate to `full_delivery` if repository evidence contradicts one
or exposes an unresolved design decision. Repository policy may require a stricter route.

The execution plan must list the host catalog and the exact model and reasoning effort for every role
that may be dispatched, including conditional specialists. Human approval covers that complete
assignment matrix. The Impact Mapper may narrow the approved role set through route and impact
classification, but it may not introduce an unapproved role, host, model, or effort. An unchanged
retry, replacement, or same-role continuation uses the existing approval, attempt fences, and remaining
budgets. Only a material change to scope, authority, route, role set, host, model, or effort returns
to preflight for a new plan and approval. Never reset a budget by relabeling a retry as a new task.
For every repository implementation intended for delivery, the human-facing plan must list the Pull
Request Engineer assignment, exact repository, remote, base, head, and allowed non-force publication
actions. Implementation authorization plus initial plan approval covers those actions after all gates;
do not seek another publication approval. Cleanup remains conditional on its separately approved
destructive authority. Neither instruction-level assignment changes engine topology or ledger state.
By default, the same approved human-facing plan must carry the bounded helper
allowance described above. The helper-enabled task and plan use schema v3 and bind the allowance
reference/hash. This remains separate from execution-plan v2 reviewer delegation and adds no graph
role, topology, ledger state, scheduler, executor, or token reducer.

Then apply these route rules:

- Every initial design route, design `REVISE`, and delivery `REDESIGN` creates the same-generation
  research pair before its next Tech Lead. Advisory and initial fast-path routes remain direct.
- A fast-path or delivery-only `REDESIGN` runs fresh design gates, then returns to a fresh Senior
  Engineer and delivery generation without changing the immutable route floor.

Treat repository routing as authoritative when it requires a stricter route.

Critical delivery tasks are engine-forced to `full_delivery` and must include the
`security_privacy` impact tag. The impact mapper cannot remove that floor.

## Execute the approved route

Read only the relevant section of [Workflow operations](references/workflow-operations.md) before
creating a brief, running a design or repair loop, coordinating delivery review, or closing. The
reference contains detailed packets and mechanics; the gates below remain mandatory.

Every initial design generation, design `REVISE`, and delivery `REDESIGN` runs the fixed
architecture/validation research pair before the next Tech Lead. The Supervisor seals that evidence,
then the Tech Lead designs. The Architect and required specialists independently approve, revise, or
block. Limit design to two revision rounds. After the second unsuccessful revision, block and return
the unresolved decision to the Supervisor or human. Begin implementation only after required design approval.

The Senior Engineer is the sole source/test writer. Every implementation and repair applies
`clean-code` and `clean-architecture-code`, preserves unrelated work, runs approved focused checks,
and reports material design deviations before editing past the approved design. It never publishes.

At a stable checkpoint, run a fresh Code Reviewer and Test Engineer independently, in parallel only
when resource assessment permits. The Code Reviewer applies `clean-code-review` and
`clean-architecture-review`. Neither gate repairs its own findings. Route one consolidated repair
packet to the Senior Engineer, then return affected findings to their originating gate. Limit delivery
to three repair rounds; material design or scope changes return to design or the human.

## Publication

For every repository implementation intended for delivery, read
[Publication contract](references/publication.md) fully while preparing the plan and again before
publication or cleanup. The approved plan names the Pull Request Engineer assignment, exact
repository, remote, base, head, allowed non-force actions, and generated-artifact locations.
Implementation authorization plus initial plan approval covers those exact actions after all gates;
do not seek another publication approval.

A fresh execution-plan-authorized Pull Request Engineer is the sole commit, push, PR, and worktree
cleanup mutator. The Supervisor verifies its evidence. Successful delivery requires exactly one
review-ready PR created or the exact existing PR updated and verified. A publication blocker means
incomplete delivery. Cleanup needs separate destructive authority and the stricter cleanliness rules
in the publication contract.

## Closure invariants

Before closing, verify every acceptance criterion has evidence, the approved design still matches,
all blocking and major findings are resolved, required checks pass, unrelated failures are separated,
and rollout, rollback, approval, and publication conditions are satisfied. Report observed
input/output/total tokens and coverage when available; otherwise say unavailable. The final response
may add tokens after the last checkpoint.

Keep one source/test writer and assess dependencies plus writable paths, mutable state, build output,
databases, ports, devices, and constrained services before each fan-out. Serialize conflicts. Start
independent reviewers/specialists from verified artifacts and a stable checkpoint, never worker
reasoning or desired conclusions. Research nodes remain evidence-only and cannot write, test, decide,
create findings, or spawn children.

Minimum handoffs are the task brief, every research or design artifact required by the selected route,
the implementation report with skill usage, independent code review with skill usage, test report,
and closure evidence. Helper packets use the canonical profile plus registered lifecycle and never
replace a parent decision. Conditional reviewer fan-out uses the frozen preliminary/request contracts;
the Supervisor alone validates, dispatches, seals, and consolidates it.
