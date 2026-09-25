# Reusable economy helpers

This is the canonical allowance, reservation, and lifecycle contract for host-only direct helpers.
Read the relevant section before approving or invoking a helper. Child behavior stays in
[evidence_scout.toml](../profile-agents/evidence_scout.toml) and
[validation_executor.toml](../profile-agents/validation_executor.toml); load only the selected
profile and do not copy its prompt into an allowance or request.

Helper allowances are enabled by default in new execution plans for eligible parents. Actual helper
sessions run when useful work can be delegated within the approved allowance. They are never graph nodes, reviewer-fanout members, schedulable
work, mandatory gates, decision makers, or source/test writers. The Supervisor remains the only
ledger mutator. The register never calls `graphctl`, executes a command, spawns an agent, mints an
approval, or aggregates token usage.

## Parent eligibility

| Parent profile | evidence_scout | validation_executor |
| --- | --- | --- |
| tech_lead | yes | no |
| software_architect | yes | no |
| senior_engineer | yes | yes |
| code_reviewer | yes | no |
| test_engineer | yes | yes |
| security_reviewer | yes | no |

All other roles, fixed researchers, specialists, Pull Request Engineer, and reviewer-fanout children
have no helper authority. Helpers cannot spawn children. Parents retain interpretation, findings,
implementation, test strategy, and terminal decisions. The Senior Engineer stays the sole source/test
writer; the Test Engineer stays read-only and owns independent verification.

## Approve and bind an allowance

Prepare a bounded allowance as part of the initial plan, without asking for a separate helper opt-in.
Include Evidence Scout for eligible planned parents and Validation Executor for planned Senior
Engineer/Test Engineer roles with exact permitted commands. Choose finite task-sized child,
concurrency, command, time, output, and read limits; do not treat the example below as universal
limits. Parents default to delegating useful independent work, while retaining trivial lookups.
Honor explicit opt-out, stricter policy, and unsupported host capabilities. Record the reason when
omitting an allowance or blocking dispatch. This planning default grants no authority before plan
approval, changes no historical plan, and bypasses no preflight, reservation, or confinement check.

New deterministic registration requires task-brief and execution-plan schema version 3. The task
attaches exactly one bounded repository JSON reference and SHA-256:

```json
{"helper_allowance":{"ref":"repo:docs/helper-allowance.json","sha256":"<64 lowercase hex>"}}
```

Hash the allowance first. The plan then binds the same reference/hash and computes its plan digest.
The registry key binds canonical state-root and repository identities plus the run ID. Its immutable
context binds the plan digest, allowance reference and source-byte digest, normalized
allowance/observation digests, and derived register path. A changed plan or allowance within the same
run reaches the same register and fails initialization rather than receiving unused budgets.
The allowance therefore omits its enclosing plan digest, avoiding a circular hash.

Task/plan v1 and v2 bytes and reconstruction remain unchanged and grant zero authority through this
new register. Loading new source does not rewrite or revoke a historical run's separately approved
instruction-level contract. Opting into the register requires an approved new v3 plan.

The allowance has schema version 1, one `allowance_id`, the `run_id`, assignments, shared limits,
and resources. Each assignment names:

- one eligible `parent_role` and permitted `helper_role`;
- `contract_revision: 1`, exact model and reasoning effort;
- a redacted `parent_capabilities` ceiling copied from the approved parent envelope's effective
  task/policy/role intersection;
- approved filesystem `scope_refs` and exact executor command objects;
- `checkpoint_policy: "observed_repository_state"`;
- mandatory `resource_keys`, required observed host capabilities, and per-assignment limits.

Limits use the exact keys `children`, `concurrency`, `commands`, `time_seconds`,
`output_tokens`, and `file_reads`. Resource entries use an opaque key and positive capacity.
Evidence Scout commands must be empty. Validation Executor commands contain an opaque command ID,
nonempty exact argv, and a timeout from 1 through 3600 seconds. Never authorize a generic shell,
wildcard, or inferred argument.

Every scope must fit a parent `filesystem_read/read` capability, and every executor command ID must
fit a parent `command/run` capability. Graph initialization recomputes the parent's effective
task/policy/role intersection and rejects declared capabilities outside it. Register initialization
and every later read validate scopes and commands against the immutable declared ceiling. The
Supervisor remains responsible for preparing the allowance, but a broader self-declaration cannot
pass graph initialization. The standalone register does not read ledger control metadata or
authenticate the approval source, so it assumes the supplied plan already passed graph validation.

A minimal Validation Executor allowance assignment looks like this inside the top-level allowance:

```json
{
  "schema_version":1,
  "allowance_id":"helpers-1",
  "run_id":"RUN-1",
  "assignments":[{
    "assignment_id":"senior-validation",
    "parent_role":"senior_engineer",
    "helper_role":"validation_executor",
    "contract_revision":1,
    "model":"gpt-6-luna",
    "reasoning_effort":"max",
    "parent_capabilities":[
      {"effect":"filesystem_read","action":"read","target_ref":"repo:src/"},
      {"effect":"filesystem_read","action":"read","target_ref":"repo:tests/"},
      {"effect":"filesystem_read","action":"read","target_ref":"repo:docs/checkpoints/"},
      {"effect":"command","action":"run","target_ref":"focused-tests"}
    ],
    "scope_refs":["repo:src/","repo:tests/","repo:docs/checkpoints/"],
    "commands":[{"command_id":"focused-tests","argv":["python","-m","unittest","tests.test_feature"],"timeout_seconds":600}],
    "checkpoint_policy":"observed_repository_state",
    "resource_keys":["worktree-build"],
    "required_host_capabilities":["fresh_model_effort_selection","filesystem_confinement","tool_confinement","command_confinement"],
    "limits":{"children":1,"concurrency":1,"commands":1,"time_seconds":900,"output_tokens":4000,"file_reads":12}
  }],
  "shared_limits":{"children":1,"concurrency":1,"commands":1,"time_seconds":900,"output_tokens":4000,"file_reads":12},
  "resources":[{"key":"worktree-build","capacity":1}]
}
```

The selected host catalog bounds supported assignment values, separately from recommendations.
Codex revision 5 suggests GPT-6 Luna/max; revisions 3 and 4 retain their earlier Luna/low
suggestions. Claude suggests Sonnet 5/low and Cursor suggests Gemini 3.8 Flash/low.
Use the preview's helper recommendation or a human-selected supported alternative, including Sol
on Codex. Bind that exact pair in the allowance before approval; graph-node overrides do not change
the allowance. Cooperative host observation must verify the exact pair. Never silently substitute,
inherit, or retune an unavailable assignment; propose an alternative for approval instead.

## Observe host capability honestly

Initialization consumes a separate bounded host-observation JSON file. It records the host ID,
observation time, source, uncertainty, exact supported model/effort pairs, and these capability
statuses:

- `fresh_model_effort_selection`;
- `filesystem_confinement`;
- `tool_confinement`;
- `command_confinement` for Validation Executor.

Each entry retains `verified`, `unverified`, or `unavailable`, plus its source and uncertainty.
A caller/prompt/self-report label cannot satisfy a required restriction. Even a host API provenance
label remains cooperative trusted-caller evidence, not authenticated runtime proof. The caller must
actually verify that source. Missing, unavailable, unverified, or assignment-mismatched requirements
return `BLOCKED_UNSUPPORTED` before reservation.

The maintenance session used for this example exposed fresh model/effort selection but no per-child
filesystem, tool, or command confinement, so its affected preflights remain
`BLOCKED_UNSUPPORTED`. Discover capabilities afresh on every actual host; a named profile or
installed contract does not confer those restrictions. Do not claim live helper enforcement,
H01-H15 success, or savings from static tests.

## Cooperative test mode

Use this mode only when the user explicitly requests a helper evaluation on a disposable local
fixture. Normal allowances remain schema version 1 and retain every confinement requirement.
For a new approved test plan, use allowance schema version 2 and add:

```json
"test_mode": {
  "disposable_repository": "<absolute resolved path to the disposable fixture>",
  "acknowledge_unenforced_isolation": true,
  "external_effects": "mocked"
}
```

Keep all other allowance fields, including required host capabilities. Bind the complete allowance
hash in the task/plan v3 attachment before approval. There is no per-call bypass flag, environment
switch, or migration of existing runs. The register checks the declared path against the actual
repository on initialization and every read/mutation. It must be strictly beneath the operating
system temporary directory, must not overlap the running skill installation, and cannot be a
linked Git worktree. This catches accidental targets; it does not prove the fixture is harmless.
The Supervisor must create a fresh fixture containing only synthetic/non-sensitive data, with no
real remotes or credentials, and keep publication, cleanup, profile changes, and external effects
mocked. Use approved exact local commands only.

Test preflight waives only the filesystem, tool, and command confinement requirements. Preserve
their observed `unavailable` or `unverified` values. Exact model/effort availability and fresh model
selection remain required, as do eligibility, scope, commands, checkpoint binding, resource limits,
reservation/replay protection, and settlement. Every normal register response, including
blocked preflights and replay responses, carries `execution_mode: "cooperative_test"`,
`production_evidence: false`, and the `unenforced_capabilities` list.

Snapshot fixture files before dispatch and compare them afterward, excluding only explicitly
approved generated outputs. Settle unexpected mutations as failed, retain evidence, and stop;
never silently restore or erase evidence. Helpers still receive bounded instructions and cannot
spawn children. These checks detect some mistakes after execution; they cannot prevent unauthorized
reads, out-of-scope writes, or external actions. Report live test behavior separately from strict
host-confinement verification and production acceptance. Passing a test does not enable strict mode.

## Initialize and operate the register

The Supervisor initializes one register under the selected absolute host state root, which defaults
to `<skill>/state`. It gives parents
only the returned register path and redacted context. Keep allowance, plan, observation, and register
files bounded and free of secrets.

```powershell
python scripts/helper-register.py initialize --state-root C:\graph-state --repo C:\work\project --run-id RUN-1 --plan C:\work\project\docs\plan.json --allowance C:\work\project\docs\helper-allowance.json --host-observation C:\evidence\host-observation.json
python scripts/helper-register.py preflight --register <returned-path> --context C:\evidence\helper-context.json --request C:\evidence\helper-request.json
python scripts/helper-register.py reserve --register <returned-path> --context C:\evidence\helper-context.json --request C:\evidence\helper-request.json
python scripts/helper-register.py settle --register <returned-path> --context C:\evidence\helper-context.json --settlement C:\evidence\helper-settlement.json
python scripts/helper-register.py status --register <returned-path> --context C:\evidence\helper-context.json
```

A request declares one unique request ID, assignment ID, parent/helper roles, contract revision,
exact assignment, a subset of approved scopes, exact selected commands, current checkpoint reference,
all allowance-owned resource keys, and requested time/output/read budgets. Example:

```json
{
  "schema_version":1,
  "request_id":"validate-1",
  "assignment_id":"senior-validation",
  "parent_role":"senior_engineer",
  "helper_role":"validation_executor",
  "contract_revision":1,
  "model":"gpt-6-luna",
  "reasoning_effort":"max",
  "scope_refs":["repo:src/","repo:tests/","repo:docs/checkpoints/"],
  "commands":[{"command_id":"focused-tests","argv":["python","-m","unittest","tests.test_feature"],"timeout_seconds":600}],
  "checkpoint_ref":"repo:docs/checkpoints/implementation.json#sha256=<64 lowercase hex>",
  "resource_keys":["worktree-build"],
  "budgets":{"time_seconds":900,"output_tokens":4000,"file_reads":12}
}
```

The checkpoint is a current, content-digested observed repository-state artifact. It may describe a
dirty but paused writer checkpoint. After in-scope repairs, use a new request identity with the new
checkpoint; this does not change the immutable allowance or require another approval. Do not run
validation while relevant files are changing.

Reserve validates again under an exclusive cross-process OS lock and atomically fsyncs/replaces the
register. It reserves child, concurrency, resource, command, time, output, and read limits together.
Every assignment resource key is mandatory, so a caller cannot omit a key to bypass serialization.
An identical request replays; the same ID with different assignment, scope, or other content
conflicts. Reserve resolves that durable identity before rechecking a changed or deleted checkpoint,
so replay acknowledges the prior reservation. It never authorizes another child dispatch. New IDs
receive the full current preflight. The one run-bound registry prevents cooperative callers from
resetting budgets by choosing another name or path.

Settlement records terminal state, evidence references, uncertainty, a hashed session identity, and
a usage reference or null. It releases only active concurrency and resources. Child, command, time,
output, and read reservations remain consumed after success, failure, cancellation, or replacement.
The register retains usage references only; existing session reporting remains the usage authority.

Every operation rechecks the bound register path plus current state-root and repository filesystem
identities. Mutating operations perform those side-effect-free checks before opening the bound lock,
then revalidate the record while holding it. The register protects against accidental cross-root use,
races, and interrupted cooperative writes. It cannot stop a malicious filesystem writer who can
rewrite both record and trusted context, a fabricated root/observation/approval, or external edits.
Caller evidence does not prove confinement. Preserve this limitation in status and checkpoint
reporting.

## Dispatch and evidence

Before dispatch, announce the concrete child identity, bounded scope, exact approved model, and exact
effort. Pass minimal fresh context: question or exact command, approved paths, current checkpoint,
assignment, budgets, expected evidence, and only the selected canonical profile. Review parents do
not pass tentative findings or desired conclusions as facts.

Settle every terminal attempt. Failures and equivalent replacements do not refund limits. Stop at
exhaustion or ambiguity and return evidence to the parent. Preserve exact errors, identifiers,
contradictions, provenance, uncertainty, and truncation while keeping full logs only in approved
locations. Report helper usage separately from parent and graph-branch usage; unavailable telemetry
stays unavailable. Use [Behavioral evaluations](behavioral-evaluations.md) for live comparisons and
never infer savings from catalog choice, elapsed time, or API token prices.
