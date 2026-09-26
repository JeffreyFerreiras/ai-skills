# Skill behavior evaluation

These scenarios test decisions that structural lint and script unit tests cannot establish. Run them in a disposable workspace with synthetic files, a temporary HOME/CODEX_HOME, and no real credentials or external write tools. Do not use installed profiles or a production vault.

For each case in [cases.json](cases.json), prepare the fixture and give a fresh evaluation session only the user prompt, relevant skill, repository instructions, and fixture. Keep the expected outcome out of the session. Record the model, supported effort setting, skill revision, tool calls, questions, exit status, and before/after file hashes. Do not infer behavior from the final answer alone.

An independent evaluator compares the trace and artifacts with the expected outcome. Mark every requirement pass, fail, or unverified and cite the exact tool event or file diff. An unverified requirement cannot pass. Record unauthorized writes, unnecessary blocking questions, wrong skill selection, unfinished authorized work, and irrelevant tests separately. A mid-turn case requires delivering the correction while work is active.

The C# review grader under `skills/clean-code-review` checks structural matching only. Its `passed` field does not certify that the prose demonstrates the actual defect. Independently inspect each claimed trigger, source line, impact, and remediation against the fixture; generic prose with the correct principle/path must fail semantic assessment. Preserve the grader's answer-key isolation.

Run these evaluations when changing instruction scope or routing and retain trace evidence with the evaluation report. Repository unit tests do not run a live model and must not be reported as behavioral success. Do not replace semantic assessment with exact matches on skill wording.

This is a manual evaluation protocol; the repository does not include a general live-model runner. Add a positive request, a near miss, and an explicit exclusion when evaluating a changed description. Report unrun cases as unverified rather than inferring success from a structural check.

Keep reports outside skill discovery roots. Include the case ID, fixture and skill hashes, host/model/effort, trace location, artifact diff, each requirement's pass/fail/unverified result, and evidence for that result. Use a separate fresh selection session with the relevant catalog when testing routing; preloading only the expected skill cannot demonstrate correct selection.

See [authoring and validation](../../docs/authoring.md#behavioral-evaluation) for how these evaluations fit the repository quality gates.
