"""Run the bounded original source-capture corpus on an owned Git fixture."""

import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile


def seed_manifest(root):
    files = []
    pending = [root]
    while pending:
        directory = pending.pop()
        for path in sorted(directory.iterdir()):
            if path.is_symlink() or path.is_junction():
                raise RuntimeError("seed fixture contains a reparse path")
            if path.is_dir():
                pending.append(path)
            elif path.is_file():
                data = path.read_bytes()
                files.append({"path": path.relative_to(root).as_posix(), "size": len(data),
                              "sha256": hashlib.sha256(data).hexdigest()})
            else:
                raise RuntimeError("seed fixture contains an unsupported file type")
    return sorted(files, key=lambda item: item["path"])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact-root", type=pathlib.Path, required=True)
    args = parser.parse_args()
    artifact = args.artifact_root.resolve(strict=True)
    fixtures = (artifact / "fixtures").resolve(strict=True)
    output = artifact / "parity" / "source-capture-01"
    output.mkdir(parents=True, exist_ok=False)
    environment_before = {key: os.environ.get(key) for key in ("TEMP", "TMP", "TMPDIR")}
    for key in environment_before:
        os.environ[key] = str(fixtures)
    root = pathlib.Path(tempfile.mkdtemp(prefix="source-capture-reference-", dir=fixtures)).resolve(strict=True)
    repository = root / "repository"
    repository.mkdir()
    if root.parent != fixtures or not repository.resolve(strict=True).is_relative_to(root):
        raise RuntimeError("source-capture fixture containment failed")
    for key in ("GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES"):
        if key in os.environ:
            raise RuntimeError("source-capture fixture requires absent " + key)
    package = pathlib.Path(__file__).resolve().parents[3] / "skills" / "software-engineering-graph"
    sys.path.insert(0, str(package))
    from graph_engine import checks

    original_git = checks._bounded_git
    current_git = []

    def observed_git(repo, arguments, deadline, output_budget=None):
        record = {"argv": ["git", *arguments], "cwd": str(repo), "deadline_diagnostic": deadline,
                  "budget_before": None if output_budget is None else output_budget[0]}
        current_git.append(record)
        try:
            result = original_git(repo, arguments, deadline, output_budget)
            record.update({"kind": "returned", "stdout_hex": result.hex(), "stderr": "DEVNULL"})
            return result
        except Exception as error:
            traceback = error.__traceback__
            chunks = None
            while traceback is not None:
                if traceback.tb_frame.f_code is original_git.__code__:
                    chunks = traceback.tb_frame.f_locals.get("chunks")
                    break
                traceback = traceback.tb_next
            captured = b"".join(chunks) if isinstance(chunks, list) and all(isinstance(chunk, bytes) for chunk in chunks) else None
            record.update({"kind": "exception", "class": type(error).__name__, "message": str(error),
                           "stdout_hex": None if captured is None else captured.hex(),
                           "stdout_note": "actual joined reader chunks from the original helper's exception frame" if captured is not None else "no captured reader chunks available in original helper frame",
                           "stderr": "DEVNULL"})
            raise
        finally:
            record["budget_after"] = None if output_budget is None else output_budget[0]

    def write(relative, data):
        path = repository / relative
        if path.is_absolute() and not path.resolve(strict=False).is_relative_to(repository.resolve(strict=True)):
            raise RuntimeError("source-capture action outside owned repository")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)

    def action(relative, data):
        return {"path": relative, "bytes_hex": data.hex()}

    outputs = [{"path": "docs/untracked.txt", "purpose": "review_report",
                "producer_node_key": "code_reviewer", "artifact_kind": "delivery_review"}]
    cases = [
        {"name": "absent_git", "roots": ["docs/"]},
        {"name": "initial", "roots": ["docs/"], "initialize_git": True},
        {"name": "untracked_first", "roots": ["docs/"], "actions": [action("docs/untracked.txt", b"first")]},
        {"name": "untracked_second", "roots": ["docs/"], "actions": [action("docs/untracked.txt", b"second")]},
        {"name": "unchanged_repeat", "roots": ["docs/"]},
        {"name": "tracked_edit", "roots": ["docs/"], "actions": [action("docs/engineering-graph.md", b"# Changed tracked source\r\n")]},
        {"name": "ignored_declared_input_without_exact_read_authority", "roots": ["docs/"],
         "known_inputs": ["docs/ignored.txt"], "actions": [action(".gitignore", b"docs/ignored.txt\n"), action("docs/ignored.txt", b"synthetic ignored source")]},
        {"name": "ignored_exact_read_authority", "roots": ["docs/ignored.txt"], "known_inputs": ["docs/ignored.txt"]},
        {"name": "missing_exact_target", "roots": ["docs/missing.txt"]},
        {"name": "generated_output_exclusion", "roots": ["docs/"], "outputs": outputs},
        {"name": "literal_bracket_target", "roots": ["docs/literal[1].txt"], "actions": [action("docs/literal[1].txt", b"literal brackets")]},
        {"name": "empty_roots", "roots": []},
    ]
    if len(cases) > 12:
        raise RuntimeError("source-capture call bound exceeded before calls")
    content_bytes = sum(len(bytes.fromhex(item["bytes_hex"])) for case in cases for item in case.get("actions", []))
    if content_bytes > 1024 * 1024:
        raise RuntimeError("source-capture generated content bound exceeded")
    initial = b"# Native source fixture\r\n"
    setup = []
    records = []
    completed = False
    checks._bounded_git = observed_git
    try:
        write("docs/engineering-graph.md", initial)
        for case in cases:
            if case.get("initialize_git"):
                for arguments in [("init",), ("add", "docs/engineering-graph.md"),
                                  ("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-m", "fixture")]:
                    completed = subprocess.run(["git", *arguments], cwd=repository, capture_output=True, check=False)
                    setup.append({"argv": ["git", *arguments], "cwd": str(repository), "exit": completed.returncode,
                                  "stdout_hex": completed.stdout.hex(), "stderr_hex": completed.stderr.hex()})
                    if completed.returncode:
                        raise RuntimeError("owned Git fixture setup failed")
                    if not (repository / ".git").resolve(strict=True).is_relative_to(repository.resolve(strict=True)):
                        raise RuntimeError("owned .git containment failed")
                seed_manifest(repository)
                shutil.copytree(repository, output / "seed-repository", symlinks=True)
            for item in case.get("actions", []):
                write(item["path"], bytes.fromhex(item["bytes_hex"]))
            current_git = []
            try:
                result = checks.capture_source(repository, {"denied_patterns": []}, case["roots"],
                    case.get("outputs", ()), case.get("known_inputs", ()))
                outcome = {"kind": "returned", "value": result}
            except Exception as error:
                outcome = {"kind": "exception", "class": type(error).__name__, "message": str(error)}
            records.append({"case": case, "expected": outcome, "git_reads": current_git})
        bindings = {"python": pathlib.Path(sys.executable), "checks": pathlib.Path(checks.__file__), "generator": pathlib.Path(__file__)}
        report = {"calls": len(records), "limit": 12, "python_version": sys.version,
            "fixture": str(repository), "initial_file": {"path": "docs/engineering-graph.md", "bytes_hex": initial.hex()},
            "seed": str(output / "seed-repository"), "seed_files": seed_manifest(output / "seed-repository"), "setup": setup,
            "environment": {key: os.environ[key] for key in environment_before},
            "bindings": {name: {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for name, path in bindings.items()},
            "records": records}
        encoded = (json.dumps(report, ensure_ascii=True, indent=2) + "\n").encode("ascii")
        (output / "report.json").write_bytes(encoded)
        completed = True
        print(json.dumps({"calls": len(records), "sha256": hashlib.sha256(encoded).hexdigest(), "report": str(output / "report.json")}))
    finally:
        checks._bounded_git = original_git
        for key, value in environment_before.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
        if root.parent != fixtures or not root.resolve(strict=True).is_relative_to(fixtures):
            raise RuntimeError("owned source-capture teardown containment failed")
        if completed:
            from owned_fixture_cleanup import remove_owned_fixture
            remove_owned_fixture(root, fixtures)
        else:
            failure = {"completed": False, "retained_fixture": str(root), "setup": setup, "records": records}
            (output / "incomplete.json").write_bytes((json.dumps(failure, ensure_ascii=True, indent=2) + "\n").encode("ascii"))


if __name__ == "__main__":
    main()
