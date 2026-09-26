---
name: root-cause-analysis
description: Investigate why a software failure, regression, or unexpected behavior occurs. Use for root-cause analysis and evidence-backed diagnosis of a reported issue.
---

# Root Cause Analysis

Explain the causal mechanism behind the reported issue, supported by reproducible observations or reliable incident evidence. Distinguish the initiating trigger, underlying defect or condition, contributing factors, and visible symptom. An error message, recent change, or successful workaround alone does not establish the cause.

## Establish the failure

- Identify expected versus observed behavior, affected scope, and the relevant version, environment, inputs, and timing. Ask only for missing information that prevents meaningful investigation; continue useful inspection meanwhile.
- Inspect applicable instructions and repository status before running diagnostics. Use the reported failing command, test, request, or incident window to bound the investigation.
- Reproduce with the smallest safe example when practical. Otherwise use existing traces, logs, dumps, and artifacts, and state the limits of that evidence. Intermittent failures need repeated observations or a controlled trigger; a single successful run does not disprove them.

## Trace and test the cause

1. Follow the failing path through relevant callers, state transitions, configuration, and dependencies. Locate the first point where observed behavior violates the expected contract, rather than stopping at the downstream exception.
2. Compare a failing case with a known-good case where available. Check differences in code, data, configuration, environment, and timing. Treat change history as a source of hypotheses, not proof of blame.
3. Keep a small set of plausible explanations. For each serious hypothesis, identify the observation that would distinguish it from alternatives and what evidence would refute it.
4. Run the cheapest discriminating check within the authorized scope. Control one suspected factor at a time where feasible. Record the relevant command or observation and result; discard explanations contradicted by evidence.
5. Confirm the causal chain. Prefer evidence that controlling the suspected factor changes the predicted outcome while relevant conditions remain stable. A patch making a test pass can mask the defect, so verify why the original failure occurred and why the proposed correction addresses it.

Investigate multiple causes when the evidence requires them. Do not force a single cause, an arbitrary number of "whys," or a precise confidence percentage. Stop repeating checks that add no information; when access or evidence prevents resolution, report the leading hypothesis and the next discriminating check.

## Scope and fixes

Diagnosis alone does not authorize source edits or operational changes. Use read-only inspection and safe local diagnostics; place any reproduction or experiment in disposable space without altering unrelated work. Do not reset a checkout, change production configuration, restart services, or replay requests with external side effects merely to test a theory.

Recommend the smallest corrective action and a regression check tied to the demonstrated failure. Separate temporary mitigation from prevention. If the user also requested a fix, implement and validate it within that existing authorization; do not ask for the same permission again. Verify the original failing case and relevant neighboring behavior before claiming resolution.

## Report

Lead with **confirmed cause**, **likely cause**, or **unresolved**, according to the evidence. Include only what the user needs to assess the conclusion:

- **Cause and mechanism:** explain how the trigger and underlying condition produce the symptom, citing relevant files/lines or incident artifacts.
- **Evidence:** the decisive reproduction, comparison, or experiment and its result; distinguish observations from inferences and explain why credible alternatives were ruled out or remain open.
- **Corrective action:** the proposed or implemented fix, any separate mitigation, and the focused verification performed or still needed.

For an unresolved investigation, name the exact missing evidence and the next useful action. Never present a plausible theory as a proven root cause or a recommendation as an applied fix.
