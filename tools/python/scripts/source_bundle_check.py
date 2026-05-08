#!/usr/bin/env python3
"""Small tooling-only check for the public source-bundle policy."""

import json
from pathlib import Path


REQUIRED = [
    "references/hp_pump_1/hp_pump_1_source_specs.json",
    "data/pump_curves/hp_pump_1.json",
]

FORBIDDEN_SUFFIXES = {".html", ".pdf"}


def main() -> int:
    root = Path(__file__).resolve().parents[3]
    missing = [path for path in REQUIRED if not (root / path).exists()]
    forbidden = [
        path.relative_to(root).as_posix()
        for path in (root / "references").rglob("*")
        if path.is_file() and path.suffix.lower() in FORBIDDEN_SUFFIXES
    ]
    if missing:
        print(json.dumps({"ok": False, "missing": missing}, sort_keys=True))
        return 1
    if forbidden:
        print(json.dumps({"ok": False, "forbidden_reference_artifacts": forbidden}, sort_keys=True))
        return 1
    print(json.dumps({"ok": True, "checked": REQUIRED, "forbidden_reference_artifacts": []}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
