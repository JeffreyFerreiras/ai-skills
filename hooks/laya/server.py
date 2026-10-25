"""Run the upstream Laya server with one local English checkpoint."""

import json
import os
from pathlib import Path


def main():
    runtime_root = Path(__file__).resolve().parent
    config = json.loads((runtime_root / "config.json").read_text(encoding="utf-8"))
    model_path = Path(config["model_path"])
    os.environ.update({
        "HF_HOME": str(config.get("cache_path", model_path.parent / ".cache")),
        "HF_HUB_OFFLINE": "1",
        "TRANSFORMERS_OFFLINE": "1",
        "LAYA_HOST": "127.0.0.1",
        "LAYA_PORT": str(config["port"]),
        "LAYA_API_KEY": (runtime_root / "token.txt").read_text(encoding="utf-8").strip(),
        "LAYA_EXTRA_MODELS": json.dumps({"hook-english": str(model_path)}),
        "LAYA_MODELS": "hook-english",
        "LAYA_DEFAULT_MODEL": "hook-english",
        "LAYA_PRELOAD": "1",
        "LAYA_AUTO_TASK": "0",
        "LAYA_MAX_LOADED": "1",
        "LAYA_THREADS": "4",
        "LAYA_LOG_LEVEL": "warning",
    })
    device = config.get("device", "auto")
    if device == "auto":
        os.environ.pop("LAYA_DEVICE", None)
    else:
        os.environ["LAYA_DEVICE"] = device

    from laya.serve import main as serve

    serve()


if __name__ == "__main__":
    main()
