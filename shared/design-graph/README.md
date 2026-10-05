# Shared design graph

This directory owns the canonical graph manifest and search script used by `clean-code` and `clean-code-review`. Edit these sources, then run `python scripts/bundle-design-graph.py` from the repository root. Run `python scripts/bundle-design-graph.py --check` to detect missing or stale bundles without writing files.

The generator copies the source bytes into each skill's `scripts/` and `references/` directories. Each installed skill can run alone. The shared directory and generator are repository authoring tools; skills do not read them at runtime.

The `search_review_graph.py` and `review-graph.manifest.json` filenames, version 2 schema, `review-signal` kind, and existing Python names remain for compatibility. These names do not limit lookup to reviews. Keep the existing CLI and search behavior when changing these sources. Do not edit the generated skill copies directly.
