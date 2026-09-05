# Skill behavior evaluation

These scenarios test decisions that structural lint and script unit tests cannot establish. Run them in a disposable workspace with synthetic files, a temporary HOME/CODEX_HOME, and no real credentials or external write tools. Do not use installed profiles or a production vault.

For each case in `cases.json`, prepare the fixture and give a fresh evaluation session only the user prompt, relevant skill, repository instructions, and fixture. Keep the expected outcome out of the session. Record the model, supported effort setting, skill revision, tool calls, questions, exit status, and before/after file hashes. Do not infer behavior from the final answer alone.

An independent evaluator compares the trace and artifacts with the expected outcome. Mark every requirement pass, fail, or unverified and cite the exact tool event or file diff. An unverified requirement cannot pass. Record unauthorized writes, unnecessary blocking questions, wrong skill selection, unfinished authorized work, and irrelevant tests separately. A mid-turn case requires delivering the correction while work is active.

The C# review grader under `skills/code-review` checks structural matching only. Its `passed` field does not certify that the prose demonstrates the actual defect. Independently inspect each claimed trigger, source line, impact, and remediation against the fixture; generic prose with the correct principle/path must fail semantic assessment. Preserve the grader's answer-key isolation.

Run these evaluations when changing instruction scope or routing and retain trace evidence with the evaluation report. CI's deterministic tests do not run a live model and must not be reported as behavioral success. Do not replace semantic assessment with exact matches on skill wording.
