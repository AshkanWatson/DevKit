#!/usr/bin/env python
"""Validate DevKit JSON Schemas and example manifests.

Checks (docs/testing/testing-strategy.md §5):
1. Every schema in schemas/*.schema.json parses and is a valid JSON Schema.
2. Every example in schemas/examples/*.json validates against its schema
   (matched by keyword: "package" -> package.schema.json, "profile" -> profile.schema.json).
3. Every fixture in schemas/examples/invalid/ FAILS validation (negative tests).

Exit code 0 = all checks pass. Used locally and in CI.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

try:
    from jsonschema import Draft202012Validator
except ImportError:
    print("error: the 'jsonschema' package is required (pip install jsonschema)", file=sys.stderr)
    sys.exit(2)

ROOT = Path(__file__).resolve().parent.parent
SCHEMAS_DIR = ROOT / "schemas"
EXAMPLES_DIR = SCHEMAS_DIR / "examples"
INVALID_DIR = EXAMPLES_DIR / "invalid"

SCHEMA_KEYWORDS = ("package", "profile")

failures: list[str] = []


def fail(message: str) -> None:
    failures.append(message)
    print(f"FAIL: {message}")


def load_json(path: Path):
    with path.open(encoding="utf-8") as fh:
        return json.load(fh)


def main() -> int:
    schemas: dict[str, Draft202012Validator] = {}
    for schema_path in sorted(SCHEMAS_DIR.glob("*.schema.json")):
        name = schema_path.stem.replace(".schema", "")
        try:
            schema = load_json(schema_path)
            Draft202012Validator.check_schema(schema)
            schemas[name] = Draft202012Validator(schema)
            print(f"schema OK: {schema_path.relative_to(ROOT)}")
        except Exception as exc:  # noqa: BLE001 — report and continue
            fail(f"{schema_path} does not parse as a valid JSON Schema: {exc}")

    matched = 0
    for example_path in sorted(EXAMPLES_DIR.glob("*.json")):
        keyword = next((kw for kw in SCHEMA_KEYWORDS if kw in example_path.name), None)
        if keyword is None or keyword not in schemas:
            fail(f"{example_path.name}: no matching schema (expected '{SCHEMA_KEYWORDS}' in filename)")
            continue
        validator = schemas[keyword]
        try:
            example = load_json(example_path)
        except json.JSONDecodeError as exc:
            fail(f"{example_path}: invalid JSON: {exc}")
            continue
        errors = sorted(validator.iter_errors(example), key=lambda e: e.json_path)
        if errors:
            detail = "; ".join(f"{e.json_path}: {e.message}" for e in errors)
            fail(f"{example_path.name}: {len(errors)} validation error(s): {detail}")
        else:
            matched += 1
            print(f"example OK: {example_path.relative_to(ROOT)}")

    for invalid_path in sorted(INVALID_DIR.glob("*.json")):
        keyword = next((kw for kw in SCHEMA_KEYWORDS if kw in invalid_path.name), None)
        if keyword is None or keyword not in schemas:
            fail(f"{invalid_path.name}: no matching schema (expected '{SCHEMA_KEYWORDS}' in filename)")
            continue
        validator = schemas[keyword]
        try:
            fixture = load_json(invalid_path)
        except json.JSONDecodeError:
            # Malformed JSON is a valid negative fixture.
            print(f"invalid fixture OK (malformed JSON): {invalid_path.relative_to(ROOT)}")
            continue
        if validator.is_valid(fixture):
            fail(f"{invalid_path.name}: expected INVALID but validated successfully")
        else:
            print(f"invalid fixture OK: {invalid_path.relative_to(ROOT)}")

    if failures:
        print(f"\n{len(failures)} failure(s)")
        return 1
    print(f"\nAll schema and example checks passed ({matched} positive examples).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
