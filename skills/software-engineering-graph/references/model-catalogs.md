# Model catalogs

The CLI's existing `--host` field selects a catalog. `codex-astra` is the default;
`codex` is the explicit size-specific Luna/Sol option for the same Codex runtime. It uses the
existing role matrix, so some review and test assignments remain economy Luna. New `cursor` plans
retain existing assignments except for the small writer policy below.
Selecting a catalog does not switch the primary agent's actual model or install role profiles.

The default Astra catalog revision 2 applies this table at every size:

| Assignments | Model and effort |
| --- | --- |
| Impact Mapper and both design research nodes | Luna `max` |
| Tech Lead, Senior Engineer, Test Engineer | Astra `low` |
| Architect, Code Reviewer, Security Reviewer | Astra `medium` |

Unlisted advisory and specialist assignments retain the baseline size matrix, with reasoning
classes mapped to Astra. The Supervisor recommendation stays Astra `xhigh`, publication stays
Luna `max`, and Supervisor consolidation remains inherited. Sizing and graph topology are unchanged.
The seven existing role profiles match the default Astra assignments; the two helper profiles
use the selected economy assignment; installed profiles are not updated
by changing this repository. If a named profile pins
different values, use a host-supported bounded fresh agent with the approved role contract and
explicit assignment. If the host cannot honor that assignment, stop that dispatch and report the
specific mismatch; never silently fall back or claim the model was changed.

Both reusable helpers use the selected host's economy assignment unless explicitly overridden by
a verified approved allowance. See [Economy helpers](economy-helpers.md) for the canonical mapping,
host/profile verification, and failure behavior. A helper-enabled task/plan v3 binds the allowance,
but helpers add no graph assignments or ledger state.

Before plan approval, verify each exact assignment against the current host's exposed capabilities.
An approved plan is not proof of model availability. Missing Supervisor model metadata selects
advisory mode but does not cancel existing approval or block unrelated authorized preparation.
Unknown branch assignments still block that dispatch. Do not request the same approval again.

Catalog selection is frozen in the plan digest. New Astra plans include `catalog_revision: 2`;
unversioned historical Astra plans reconstruct the original size matrix without changing bytes,
digests, or approvals. Resuming retains that catalog generation; changing it requires a new plan.
Historical `codex` and `cursor` plan shapes and digests remain unchanged. New plans for both
catalogs include `catalog_revision: 2`; only the small Senior Engineer changes from economy to the
existing reasoning `medium` mapping: Sol `medium` on Codex, Grok `medium` on Cursor. Other roles and
medium/large assignments remain unchanged. Astra revision 2, including its writer at `low`, is
preserved byte-for-byte. Older engines cannot read unsupported revision 2 plans. Do not rewrite approvals to make a rollback work.
Reviewer delegation can explicitly approve Astra `medium` or `high` at weight 3, `xhigh` at 4,
or `max` at 5. Astra `low` and Sol `medium` remain unsupported for delegation. These are
dispatch-budget weights, not price estimates; token accounting is unchanged.

Official guidance checked September 5, 2026:
[Astra model](https://developers.openai.com/api/docs/models/gpt-6-astra) and
[Astra prompting guidance](https://developers.openai.com/api/docs/guides/latest-model?model=gpt-6-astra).
The published API efforts include `low`, `medium`, `high`, `xhigh`, and `max`. This graph uses
only its catalog's explicit assignments; host-specific additional efforts are not implied.
This engine coordinates agents and makes no API requests, so API transport or parameter migration
does not belong in the ledger.

Astra is the default at the user's explicit request after confirming account access.
This preference change does not establish comparative quality, latency, or cost benefits.
Use [behavioral-evaluations.md](behavioral-evaluations.md) with both catalogs under equivalent host
capabilities before making an evidence-based recommendation. Record task completion, unauthorized
effects, avoidable approval pauses, verification repeats, dispatch accuracy, elapsed time, and measured
usage. No live-model evaluation has been established by this default change.
