# Software Engineering Graph migration

The canonical skill now lives at `skills/software-engineering-graph` in this repository. It is a normal locally maintained skill; the former `external-source.json` installation pointer has been removed.

## Source and preservation

Source repository: https://github.com/JeffreyFerreiras/software-engineering-graph

Source commit: `2b358305b93ac677b45d6c22ec46b25afc965b4b` (the merged Astra-default change).

The [file inventory](software-engineering-graph-20260906.json) records all 173 tracked source files and their SHA-256 hashes. All 56 graph payload files were imported, including the engine, entrypoint, schemas, tests and fixtures, role profiles, contributor guidance, ignore rules, documentation, and license. The other 117 files are contributor-tooling installations and repository metadata retained in the old repository, not omitted graph resources.

Engine code, fixtures, schemas, role assignments, and the skill entrypoint are unchanged. Three imported documentation files were adapted: README names the new canonical location and command working directory; AGENTS identifies ai-skills as authoritative; the ledger guide makes its existing schema reference a valid relative Markdown link. One contract test now checks the canonical dispatch instructions directly instead of comparing them with the former repository's nested contributor-tooling copy. The shared test fixture resolves temporary paths so hosted Windows 8.3 aliases match the engine's resolved repository path. The inventory records each imported file's destination hash and any adaptation reason. Text hashes use Git's LF representation.

The old repository retains its complete history and links to the final standalone commit. Both repositories also had verified local Git bundles created before editing. Existing checkouts, other branches/worktrees, ignored files, and installed profiles were not overwritten by this migration.

The prior ai-skills pointer and its metadata remain recoverable at ai-skills commit `423a5ba6633e1d2f18f1e514f9ce56c063ce1296`.

## Merge order

Merge the ai-skills import PR first. Only then merge the external repository's redirect PR, so the public link resolves to the full implementation before the old default branch removes it. Neither PR merges itself.

## Validation and installation

CI runs the graph's documented focused acceptance suite from `skills/software-engineering-graph`, with bytecode disabled, and runs graph hygiene last. Repository validation checks the imported skill's metadata, references, and Python syntax.

Use the normal sync-agent-skills workflow against this repository for future installations. This migration does not synchronize existing installations. An old external resolver against the redirect repository will fail its required-file check instead of replacing an installed engine with a pointer.
