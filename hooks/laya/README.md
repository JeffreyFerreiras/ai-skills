# Laya client installation

The two Laya implementations are consolidated into the resident **LayaCodex**
service and one routing policy. See the [setup and migration guide](../../docs/laya-hook.md).

This directory contains the user-profile installer and managed subagent
instructions. Runtime clients come from `scripts/`; no second model server or
environment is installed here. Existing weights, tokens, logs, and legacy runtime
files are retained during migration.
