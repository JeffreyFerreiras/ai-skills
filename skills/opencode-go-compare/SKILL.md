---
name: opencode-go-compare
description: Compare OpenCode Go models on live price, speed, context, and benchmark data. Use when asked to build or check a comparison table for OpenCode Go models, including cost per task and benchmark scores. Do not use to choose a reasoning effort level or to delegate work through the OpenCode CLI.
---

# Compare OpenCode Go Models

Produce two comparison tables for the models that OpenCode Go lists now, then a
short reading of what they show. Pull catalog values live. Treat any model list
in this file, including the snapshot near the end, as history, never as the
catalog.

Return the main table, the benchmark table, at most four paragraphs of reading,
the catalog size, and a retrieval date on every column.

## Step 1: Enumerate the live catalog

Call the OpenCode `models` tool with an empty query, provider `opencode-go`, and
the largest page size:

```text
models({ query: "", provider: "opencode-go", limit: 100 })
```

Follow `next` with its `offset` value until it is null. Add the returned model
count and compare it with the `total` the tool reports. Report both numbers so
silent truncation is visible. If the call fails or returns nothing, stop and
report the failure. Do not fill the tables from memory.

Capture these fields for every model:

- `id` and `name`
- `released`
- `variants` (effort levels)
- `cost`: input, output, cache read, cache write, and any context tier
- `status`

Record the call time. Use it as the retrieval date for every catalog row.
Exclude any entry whose `status` is not `active`. If the user names such a
model, include it and flag the column as not active.

## Step 2: Resolve each model to a benchmark source

Pick the columns first.

- If the user names models, compare those.
- Otherwise use at most 8 columns: the 8 most recently released chat models in
  the catalog, plus any model the user names.
- Exclude models whose OpenCode Go input and output prices are both zero from
  the default set. Mention them in one line instead of giving them a column.

The 8-column cap keeps the table readable. State the cap and name any default
model that a user-named model pushed out.

Collapse contributor or free variants into one column for the base model, name
the tier used, and say which tier supplied the price. A contributor tier trades
a lower price for permission to use prompts and completions for training. Label
such a column `contributor tier`. Do not present its price or benchmark terms
as first-party terms.

For each column, open the Artificial Analysis model page. Use this slug rule:
lowercase, then replace spaces and dots with dashes. Examples only, verify
against the live page: `Kimi K3.1 Pro` becomes `kimi-k3-1-pro`, and
`DeepSeek V4.1 Flash 1017` becomes `deepseek-v4-1-flash-1017`.

If the slug does not resolve, search the AA model index and record the URL that
worked. Do not guess twice.

Read the page for a deprecation banner. When the banner names a newer release,
follow that release and use its slug instead. Record both slugs. A plain name
can resolve to an older build with a much lower score, and that error is silent.

Record the AA retrieval date for every column. Tag every cell with its source:

| Tag | Source |
| --- | --- |
| `AA` | Artificial Analysis model page |
| `C` | OpenCode Go catalog call |
| `V` | Vendor launch material |
| `I` | Independent evaluator, such as Vals AI, LMSpeed, or Design Arena |
| `D` | Derived by the source, not recomputed here |

Use vendor material and independent evaluators only where AA publishes no value.
Prefer an independent result over a vendor result for the same benchmark.

A dash means the source was read and omits the value. A fetch that fails, is
blocked, or requires login is reported as a failure with the URL and the error,
and is never rendered as a dash. If one column cannot be read at all, drop that
column and state that it was dropped and why.

## Step 3: Build the main table

Put one column per model. Put the AA retrieval date in each column header, and
the effort tier when the model exposes more than one. When a model exposes
several tiers, use the highest tier AA reports unless the user names a tier.
Note in the column header when a column's tier differs from its neighbours.

| Property | Model A (AA date, tier) | Model B (AA date, tier) |
| --- | --- | --- |
| AA Intelligence Index | | |
| AA rank | | |
| Output speed | | |
| Time to first token | | |
| Vendor price in / out | | |
| OpenCode Go price | | |
| Cache read | | |
| Cache write | | |
| Cost per AA task | | |
| Tokens per task | | |
| Parameters | | |
| Weights | | |
| Context | | |
| Effort levels | | |
| Released | | |

Fill the rows in that order.

1. **AA Intelligence Index:** the composite score as published. Add the index
   version when the page shows one.
2. **AA rank:** the rank AA displays, written as `x of y`.
3. **Output speed:** decode tokens per second. Never call it the time to a
   usable answer.
4. **Time to first token:** seconds, excluding model thinking. For a reasoning
   model, also give the time to the first answer token when it is materially
   larger, for example `1.1 (4.8 to answer)`.
5. **Vendor price in / out:** first-party list price per 1M tokens, input then
   output, for example `$2.00 / $10.00`.
6. **OpenCode Go price:** from the catalog call, in the same shape, using the
   base `cost` entry. The base entry is the item with no `tier`. When a context
   tier exists, state the threshold and the alternate input/output price in the
   cell or in a footnote under the table. Flag any cell that uses a contributor
   tier, a free tier, a context tier, or a promotion, because those terms
   differ.
7. **Cache read:** per 1M tokens from the `cost` entry used in row 6, tag `C`.
   Use a dash when the provider does not offer it. AA's cached-input price can
   differ from the catalog value. This row reports the catalog value.
8. **Cache write:** per 1M tokens from the `cost` entry used in row 6, tag `C`,
   when the provider bills it, otherwise a dash. Most providers here do not bill
   cache writes. Hiding that difference overstates the savings from repeated
   long-context work. Rows 6, 7, and 8 must use the same `cost` entry.
9. **Cost per AA task:** the weighted average cost per Intelligence Index task
   that Artificial Analysis publishes. AA derives it from input, cache-hit,
   cache-write, reasoning, and answer tokens at list prices. Tag it `D` because
   it is derived. Do not claim this skill recomputes it.
10. **Tokens per task:** output tokens only, per Intelligence Index task, written
    in millions. It cannot be multiplied by the output price to reproduce `Cost
    per AA task`. Do not attempt that arithmetic. A high value still means the
    headline price understates real spend.
11. **Parameters:** total and active for a mixture-of-experts model, for
    example `552B / 16B active`. Write `undisclosed` when the vendor does not
    publish it.
12. **Weights:** `Open`, `Open (license name)`, or `Proprietary`. Name the
    license when it is permissive.
13. **Context:** from the AA model page, tag `AA`, or from the vendor spec sheet,
    tag `V`, when AA omits it. Use a dash when neither publishes it. Do not use
    memory.
14. **Effort levels:** list the catalog `variants` values exactly as returned,
    tag `C`. A `none` variant is a no-reasoning level and must still be listed.
    Write `none exposed` only when the array is empty.
15. **Released:** ISO date from the catalog.

## Step 4: Build the benchmark table

Use the same model columns. Fill these rows in this order. These benchmarks
matter most for code review and architecture work.

| Benchmark | Model A | Model B |
| --- | --- | --- |
| AA-LCR | | |
| AA-Briefcase Elo | | |
| GDPval-AA | | |
| MRCR 512K-1M | | |
| DeepSWE v1.1 | | |
| Terminal-Bench 4.0 (AA run) | | |
| CyberGym | | |
| Vals Index | | |
| HLE | | |
| GPQA Diamond | | |
| Cost per AA task | | |

Rules for this table:

- Mark a score inherited from a predecessor model with a dagger. Use it when the
  current release has only an aggregate index published. Add a footnote that
  names the predecessor.
- Use a dash for a genuinely unpublished score. Never write zero.
- Name the harness in the cell when a benchmark has more than one run, for
  example `AA run` or `vendor run`.
- Prefer the independent run. Vendor numbers come from the vendor's own harness
  and tend to be higher. Never describe a vendor run as independent.
- The `Cost per AA task` row repeats Table 1. It is not a second measurement.

## Step 5: Write the reading

Add at most four short paragraphs after the tables.

1. Name which columns win which axis, with the values that decide it.
2. Say where the value tier sits against the intelligence tier.
3. Give the one structural insight worth acting on. Gated work, such as review
   or architecture approval, spends large input and small output. A high
   per-token price can still cost cents per gate, while the implementation loop
   needs the cheap model. Compute one gate from the table's own prices so the
   claim is checkable: `(input tokens / 1e6 * input price) + (output tokens /
   1e6 * output price)`. State the token assumptions, show the arithmetic, and
   use the cache read price for a reused prefix when the provider has one.
4. End with limits: every number has a retrieval date, vendor runs are not
   independent, and a table older than about a month should be regenerated
   rather than quoted.

Prefer a two-tier recommendation over one global winner: a cheap
high-throughput model for implementation loops, and a high-index model for
review and architecture gates.

## Data integrity rules

1. Never mix effort tiers in one column. Name the tier in the column header.
2. Never present a vendor benchmark run as independent.
3. Never present derived cost per task as a list price.
4. Never let a deprecated AA slug supply a score.
5. Never carry a number forward without its retrieval date.
6. Never compare output speed across providers as if it were the same
   measurement. AA measures the first-party API or a provider median, and those
   differ.
7. Prefer a two-tier recommendation over one global winner.

## Reference snapshot

Dated 2026-10-24. This snapshot is a sanity check to compare against, never a
substitute for the live pull. During a comparison run the snapshot is
report-only: report any difference against the live pull, and leave editing the
snapshot to a separate maintenance change.

The snapshot uses a model-per-tier view for reference, so one model can appear
twice with different tiers. That is not the Table 1 layout, which uses one
column per model at a single named tier.

| Model | AA index | Speed tok/s | TTFT s | Price in/out | Cost/task | Tokens/task | Params | Weights |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Kimi K3.1 Pro | 55 | 77.1 | 2.89 | $2.00 / $10.00 | $2.11 | 150M | 2.8T | Proprietary |
| Muse Spark 1.3 (max) | 48 | 116 | ~27 | $1.25 / $4.25 | $1.60 | 170M | undisclosed | Proprietary |
| Muse Spark 1.3 (xhigh) | 45 | 289.5 | ~19 | $1.25 / $4.25 | $1.37 | 140M | undisclosed | Proprietary |
| DeepSeek V4.1 Flash 1017 | 45 | 195.2 | 1.14 | $0.30 / $1.20 | $0.26 | 210M | 552B / 16B active | Open, MIT |
| Step 5 Preview | 44 | 224.0 | 1.18 | $1.00 / $2.70 | $1.03 | 160M | 600B / 27B active | Open, restricted |
| MiniMax M3.1 Flash | 43 | 87.2 | 1.48 | $0.15 / $0.60 | $0.22 | 180M | undisclosed | Proprietary |

- Kimi K3.1 Pro publishes only an aggregate index. Inherit its per-benchmark
  cells from K3 and mark each with a dagger.
- Muse Spark speed varies across AA surfaces, from 241 to 290 tok/s for xhigh,
  because different pages sample different windows. Report the value and the
  page it came from.
- OpenCode Go list prices can differ from vendor list prices. DeepSeek V4.1
  Flash is listed on OpenCode Go below DeepSeek's own price.

## Before returning

- Report the model count and the tool's total.
- Keep at most 8 default columns. Go beyond that cap only for models the user
  names.
- Keep both tables in the required row order.
- Put an AA retrieval date on every column.
- Label derived cost and inherited scores.
- Keep the reading to four paragraphs or fewer, with one computed gate example.
