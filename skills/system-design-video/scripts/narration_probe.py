#!/usr/bin/env python3
"""Bounded OpenVINO device discovery. Never loads a model or starts inference."""

from __future__ import annotations

import argparse
import importlib.metadata
import json
import platform
import subprocess
import sys
from pathlib import Path


def empty_report():
    versions = {}
    for package in ("openvino", "openvino-genai", "openvino-tokenizers"):
        try:
            versions[package] = importlib.metadata.version(package)
        except importlib.metadata.PackageNotFoundError:
            versions[package] = None
    return {
        "scope": "Device discovery only; no model compatibility or stability test",
        "host": platform.node(),
        "os": platform.platform(),
        "python": sys.version.split()[0],
        "packages": versions,
        "devices": [],
        "execution_device": None,
        "cpu_fallback_executed": False,
        "npu_validation": "pending",
    }


def probe(core_factory=None):
    report = empty_report()
    try:
        if core_factory is None:
            from openvino import Core
            core_factory = Core
        core = core_factory()
        for device in core.available_devices:
            item = {"id": device, "properties": {}, "property_errors": {}}
            for key in ("FULL_DEVICE_NAME", "DRIVER_VERSION"):
                try:
                    item["properties"][key] = str(core.get_property(device, key))
                except Exception as error:
                    item["property_errors"][key] = str(error)
            report["devices"].append(item)
        report["status"] = "discovered"
    except Exception as error:
        report.update(status="unavailable", error=str(error))
    return report


def bounded_probe(timeout):
    command = [sys.executable, str(Path(__file__).resolve()), "--worker"]
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=timeout)
        report = json.loads(result.stdout)
        if result.returncode not in (0, 1) or not isinstance(report, dict):
            raise ValueError(f"Discovery worker exited {result.returncode}")
        return report
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        report = empty_report()
        report.update(status="error", error=str(error))
        return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--timeout", type=int, default=15,
                        help="Discovery worker deadline in seconds, 1-60 (default: 15)")
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if not 1 <= args.timeout <= 60:
        parser.error("--timeout must be between 1 and 60 seconds")
    report = probe() if args.worker else bounded_probe(args.timeout)
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return {"discovered": 0, "unavailable": 1, "error": 2}[report["status"]]


if __name__ == "__main__":
    raise SystemExit(main())
