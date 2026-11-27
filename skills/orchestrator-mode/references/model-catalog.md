# OpenCode helper model catalog

Catalog version **1.0**. Evidence checked **2026-11-18** with OpenCode **2.3.0** using `opencode api model.list` and the online sources below. Scores use the **Artificial Analysis Intelligence Index v5.0.1**, a composite across knowledge work, coding, science, and general knowledge. The thresholds are local routing heuristics. Individual jobs still need a fit check and independent acceptance. [Leaderboard](https://artificialanalysis.ai/leaderboards/models) · [Index methodology](https://artificialanalysis.ai/methodology/intelligence-benchmarking)

## Capability floor

Classify each bounded job by the reasoning it requires after scoping it. Use the highest applicable level.

| Level | Job characteristics | Default minimum index score |
| --- | --- | ---: |
| Basic | Clear instructions, a familiar operation, and a direct check. Examples include an exact lookup or a mechanical edit. | 25 |
| Involved | Several dependent steps, local ambiguity, or a bounded change that needs reasoning about side effects. | 30 |
| Difficult | Competing hypotheses, cross-component dependencies, or subtle failures that a focused check could miss. | 40 |

For specialized jobs, specify what success on a relevant evaluation looks like before selecting a model. For example, use FrontierCode-AA 2.0 results for repository changes or Terminal-Bench 4.0 results for terminal work, with a comparable setup. When the composite score is a poor fit or unavailable, use a predeclared task-specific pass standard commensurate with the level. Use evidence for the same model release and relevant effort and setup, such as published results or independently checked outcomes from comparable jobs. Keep that standard through fallback. Recalibrate the thresholds if the index or version changes. [Benchmark definitions](https://artificialanalysis.ai/methodology/intelligence-benchmarking)

## Route snapshot

OpenCode's provider catalog maps the route IDs below to the benchmarked releases. The CLI snapshot supplies the supported effort variants shown. Listed variants omit lower efforts for brevity. The score applies to the stated benchmark effort. A row with unspecified or unavailable effort is a lead and needs job-specific evidence for the actual configuration before automatic routing. An explicit user pin uses the pin rules in the skill. [Console route mapping](https://models.opencode.ai/providers/opencode/) · [Go route mapping](https://models.opencode.ai/providers/opencode-go/)

| OpenCode route | Cost class | CLI efforts | v5.0.1 benchmark evidence |
| --- | --- | --- | --- |
| `opencode/deepseek-v4.1-flash-free` | Free | `high`, `max` | DeepSeek V4.1 Flash **1017**, 29 at `high` and 33 at `max`. Use the 1017 release mapping for this alias. [Benchmark](https://artificialanalysis.ai/models/deepseek-v4-1-flash-1017) |
| `opencode/inkling-2-small-free` | Free | `high`, `max` | Inkling 2 Small, 31 at unspecified effort. [Benchmark](https://artificialanalysis.ai/models/inkling-2-small) |
| `opencode/beam-free` | Free | `high`, `xhigh`, `max` | Beam 1.0 501B A23B, 18 at unspecified effort. [Benchmark](https://artificialanalysis.ai/models/reflection-beam) |
| `opencode/mimo-v2.6-flash-free` | Free | None listed | MiMo V2.6 Flash, 27 at unspecified effort. The route lacks a listed variant meeting Little Helper's free effort floor. [Leaderboard](https://artificialanalysis.ai/leaderboards/models) |
| `opencode-go/glm-5.3` | Paid | `high`, `max` | GLM-5.3, 32 at `max`. [Leaderboard](https://artificialanalysis.ai/leaderboards/models) |
| `opencode-go/kimi-k3.1-pro` | Paid | `high`, `max` | Kimi K3.1 Pro, 46 at unspecified effort. [Leaderboard](https://artificialanalysis.ai/leaderboards/models) |
| `opencode-go/gemini-4-argon` | Paid | `high` | Gemini 4 Argon, 50 at `high`. [Leaderboard](https://artificialanalysis.ai/leaderboards/models) |
| `opencode-go/muse-spark-1.3-contributor` | Paid | `xhigh` | Muse Spark 1.3 has no v5.0.1 `xhigh` score. The published 37 applies to `max`, which this route lacks. [Benchmark](https://artificialanalysis.ai/models/muse-spark-1-3) |

Use the snapshot for shortlisting. Before dispatch, verify the current route, alias-to-release mapping, effort, tools, limits, availability, regional eligibility, and terms for the job's data. Confirm applicable input, output, and cache charges. Some free offers expire or have training, retention, or regional conditions. Muse Spark Contributor permits training on prompts and completions. An enabled route still requires authorization for paid usage. [Console pricing, free offers, and privacy](https://opencode.ai/docs/console/models/#pricing) · [Go limits and privacy](https://opencode.ai/docs/console/go/#privacy)

Reuse catalog evidence while it fits the job and runtime. When an alias changes, an effort is missing, or a route is deprecated or no longer available, verify only the affected routes and compare results from the same benchmark version. Treat new or conflicting mappings as unknown until resolved. Keep source, checked date, benchmark version, release, route, and effort with any new result. Published benchmark latency reflects its own tasks and provider setup. Use evidence from comparable bounded jobs when available to judge the 150-second limit.
