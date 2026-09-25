# Repository contributor constraints

- The ai-skills repository is authoritative for maintained skill code. Installed profile code remains untouched unless
  separately approved work explicitly changes it. Normal graph runs may create or update policy,
  ledger state, lessons, and artifacts under the selected sibling `<profile>.local/software-engineering-graph/`
  runtime home or its explicit absolute override.
- The supported `profile-agents/` inventory is `impact_mapper`, `tech_lead`, `software_architect`,
  `senior_engineer`, `code_reviewer`, `test_engineer`, `security_reviewer`, `evidence_scout`, and
  `validation_executor`, each as one TOML. Preserve every role's responsibilities and validate this
  exact inventory. The two helpers are optional child contracts, not graph stages. Further profile
  additions require explicit scope approval.
- Retain optional specialist protocol identifiers and current executable behavior.
- Use Python 3.9 or newer and the standard library only. Add no dependency or packaging system.
- Do not track cache, bytecode, virtual environment, database, run-state, inbox, secret, environment,
  coverage, build, or temporary artifacts in the source repository. Historical installed-skill
  `policies/` and `state/` directories remain ignored; never stage or sync them.
- Run only the focused test suite explicitly enumerated in `README.md`, with
  `PYTHONDONTWRITEBYTECODE=1`. Run hygiene separately and last after final review.
- Do not mutate a profile, consumer repository, or remote system without separately approved scope,
  except writes within the selected runtime home required by an authorized graph run.
