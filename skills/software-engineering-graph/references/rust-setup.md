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

The installed `scripts/graphctl` and `scripts/helper-register` are native binaries
(`.exe` on Windows). Legacy `.py` launchers execute those binaries, not the Python
engine. Both binaries must pass `--help` smoke checks before setup publishes the
installation. No model calls, repository mutations, or live graph runs are part
of this smoke check.

Use the installed skill, not the source checkout's Python launcher. Keep generated
runtime policies and ledgers outside the installation as described in SKILL.md.
Model-provider access and execution-plan approval remain separate prerequisites
for running an actual workflow.
