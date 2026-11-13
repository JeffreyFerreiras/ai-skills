# Rust installation

Run setup from a checkout of `JeffreyFerreiras/ai-skills`. Prerequisites are Python
3.12 with Unicode 15.0 data, Git, Rust/Cargo 1.85 or newer, and a C toolchain for
bundled SQLite. Install Rust from its official rustup distribution if needed.
Setup does not install system software or change shell profiles.

```sh
python3 skills/software-engineering-graph/scripts/setup-rust.py \
  --source . --install-dir /absolute/path/to/software-engineering-graph
```

Choose a new destination; setup refuses to overwrite an existing installation.
An installation managed by a skill service must still be saved through that
service's supported workflow. Building a directory alone does not register it.

For a source-only package, add `--source-only`. This bundles the Rust sources and
verified frozen oracle files instead of compiled binaries. The first invocation
builds both native commands in a writable cache; later invocations verify the
cached binaries and reuse them. Python runs setup and launch tooling only, never
the graph engine. The first build requires the toolchain and may download locked
Cargo dependencies. The installed source directory remains unchanged.

Set an absolute `GRAPH_RUST_CACHE_HOME` to choose the cache location. Otherwise the
launcher uses `XDG_CACHE_HOME` or the user's `.cache` directory. Cache reuse lasts
only while those files remain available; a new cloud environment may rebuild.
The launcher supplies `GRAPH_RUST_SKILL_ROOT` to keep resource lookup and default
runtime-state paths tied to the installed skill rather than the build cache.

Setup restores the historical Unicode artifact byte-for-byte and requires its
frozen SHA-256 before using it. This restores existing bytes; it does not claim
that the current interpreter is the historical oracle. It leaves the original
generator and frozen manifest unchanged. To use an existing verified artifact,
pass `--unicode-input /absolute/path/to/unicode-tables.bin` instead.

Setup obtains the frozen oracle source revision when absent, verifies every
module hash, and builds in an isolated staging tree with Cargo's locked dependency
versions. Network access may be needed for the oracle revision and Cargo crates.
Source hashes and compatibility hashes must match; never work around a mismatch
by editing a manifest or accepting newly generated input.

In immediate-build mode, installed `scripts/graphctl` and `scripts/helper-register`
are native binaries (`.exe` on Windows). In source-only mode they are build/cache
launchers. Legacy `.py` entry points also execute Rust, not the Python engine.
Both binaries must pass `--help` smoke checks before an immediate-build installation
is published, or before a source-only first build is marked reusable. No model
calls, repository mutations, or live graph runs are part of this smoke check.

Use the installed skill, not the source checkout's Python launcher. Keep generated
runtime policies and ledgers outside the installation as described in SKILL.md.
Model-provider access and execution-plan approval remain separate prerequisites
for running an actual workflow.
