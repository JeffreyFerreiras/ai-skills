# Subagent routing

Choose the subagent by the shape of the task. This table is the routing rule.
If no row fits, do the work in the primary session instead of delegating.

| Task shape | Subagent | What it does |
| --- | --- | --- |
| Find files, symbols, or where something lives. Answer "how does this work" about existing code. | `explore` | Read-only search. Returns file references and answers. Cannot edit. |
| Change the repository: implement, refactor, fix, add tests, run commands. Any task with several dependent steps. | `general` | Full tools. Edits files, runs commands, and can split work in parallel. |
| Judge a change that already exists: a diff, a commit range, or finished work. | `reviewer` | Read-only review. Cannot edit. Reports defects with file and line references. |

Apply these rules when delegating:

- Pick one subagent per task. Do not send the same question to `explore` and then to `general`.
- Use `explore` when the answer is a fact about the code. Use `general` when the answer is a change to the code.
- Use `reviewer` only after work exists. Do not use it to plan, explore, or implement.
- `reviewer` cannot edit or run commands beyond read-only git inspection. Apply its findings in the primary session.
- Pass the task, the relevant paths, and the expected result. A vague delegation wastes the model you paid for.
