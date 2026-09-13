# Reusable economy helpers

This is the canonical allowance, reservation, and lifecycle contract for host-only direct helpers.
Read the relevant section before approving or invoking a helper. Child behavior stays in
[evidence_scout.toml](../profile-agents/evidence_scout.toml) and
[validation_executor.toml](../profile-agents/validation_executor.toml); load only the selected
profile and do not copy its prompt into an allowance or request.

Helpers are optional host sessions. They are never graph nodes, reviewer-fanout members, schedulable
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

New deterministic registration requires task-brief and execution-plan schema version 3. The task
attaches exactly one bounded repository JSON reference and SHA-256:

```json
{"helper_allowance":{"ref":"repo:docs/helper-allowance.json","sha256":"<64 lowercase hex>"}}
```

Hash the allowance first. The plan then binds the same reference/hash and computes its plan digest.
The registry key binds canonical state-root and repository identities, run ID, plan digest, and
allowance source-byte digest. Its immutable context also binds the allowance reference, source-byte
digests, canonical normalized allowance/observation digests, and derived register path.
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
fit a parent `command/run` capability. The register validates that ceiling during initialization and
again whenever it reads the record. The Supervisor remains responsible for copying it faithfully from
the approved parent envelope; task-wide authority alone is insufficient because the role ceiling may
be narrower. The register does not read ledger control metadata or authenticate the approval source.

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
    "model":"gpt-5.6-luna",
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

The selected host catalog bounds assignment values. Codex and Astra default helpers resolve to
Luna/max; Cursor defaults resolve to Composer/high. An explicitly approved alternative is permitted
only when it is a recognized pair in that selected catalog and cooperative host observation evidence
verifies the exact pair. Never substitute, inherit, or retune an unavailable assignment.

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

## Initialize and operate the register

The Supervisor initializes one register under an explicit absolute host state root. It gives parents
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
  "model":"gpt-5.6-luna",
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
