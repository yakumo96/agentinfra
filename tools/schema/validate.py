"""Check every schema under schemas/. Exit 1 on the first class of problems found.

    python3 tools/schema/validate.py
"""
from __future__ import annotations

import sys
from pathlib import Path

from jsonschema import Draft202012Validator

sys.path.insert(0, str(Path(__file__).parent))
import schemalib as sl  # noqa: E402


def iter_refs(node, at=""):
    if isinstance(node, dict):
        if "$ref" in node:
            yield at, node["$ref"]
        for k, v in node.items():
            yield from iter_refs(v, f"{at}/{k}")
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from iter_refs(v, f"{at}/{i}")


def check() -> list[str]:
    problems: list[str] = []
    meta = sl.load_json(sl.SCHEMAS / "tool-definition.schema.json")
    meta_v = Draft202012Validator(meta)
    defs = sl.load_defs()
    errors = sl.load_errors()

    # errors.json and the ErrorCode enum agree
    codes = [e["code"] for e in errors]
    if len(set(codes)) != len(codes):
        problems.append("errors.json: duplicate codes")
    if defs["ErrorCode"]["enum"] != codes:
        problems.append("defs.ErrorCode.enum must equal the codes in errors.json, in order")

    # defs refer only to defs
    for at, ref in iter_refs(defs):
        if not ref.startswith(sl.DEFS_REF) or sl.ref_name(ref) not in defs:
            problems.append(f"defs{at}: unresolved $ref {ref}")

    names: dict[str, Path] = {}
    used: set[str] = set()
    for tool in sl.load_tools():
        rel = tool.path.relative_to(sl.REPO)
        doc = tool.doc
        meta_errors = [f"{rel}: {e.json_path}: {e.message}" for e in meta_v.iter_errors(doc)]
        if meta_errors:
            problems.extend(meta_errors)
            continue
        if tool.path.stem != doc["name"]:
            problems.append(f"{rel}: file name must equal tool name {doc['name']!r}")
        if tool.path.parent.name != doc["group"]:
            problems.append(f"{rel}: directory must equal group {doc['group']!r}")
        if doc["name"] in names:
            problems.append(f"{rel}: duplicate tool name (also {names[doc['name']]})")
        names[doc["name"]] = rel

        inp = doc["inputSchema"]
        has_key = "idempotency_key" in inp.get("properties", {})
        required_key = "idempotency_key" in inp.get("required", [])
        if doc["mutates"] and not (has_key and required_key):
            problems.append(f"{rel}: mutating tool must require idempotency_key")
        if not doc["mutates"] and has_key:
            problems.append(f"{rel}: read-only tool must not take idempotency_key")
        if doc.get("harness_builtin") and "worker" not in doc["roles"]:
            problems.append(f"{rel}: harness built-in targets must be callable by workers")

        for part in ("inputSchema", "outputSchema"):
            for at, ref in iter_refs(doc[part]):
                if not ref.startswith(sl.DEFS_REF) or sl.ref_name(ref) not in defs:
                    problems.append(f"{rel}: {part}{at}: unresolved $ref {ref}")
                else:
                    used.add(sl.ref_name(ref))
            try:
                Draft202012Validator.check_schema(sl.inline_refs(doc[part], defs))
            except Exception as exc:  # SchemaError or recursion
                problems.append(f"{rel}: {part}: {exc}")

    # transitive use through defs; the error envelope and roles are used by every server and client
    used |= {"Error", "ErrorCode", "Role"}
    frontier = set(used)
    while frontier:
        nxt = set()
        for name in frontier:
            for _, ref in iter_refs(defs[name]):
                n = sl.ref_name(ref)
                if n not in used:
                    used.add(n)
                    nxt.add(n)
        frontier = nxt
    for name in sorted(set(defs) - used):
        problems.append(f"defs.{name}: defined but never used")

    problems.extend(check_examples(defs))
    return problems


def check_examples(defs: dict) -> list[str]:
    """schemas/examples/<tool>/{valid,invalid}/*.json against the bundled input schema."""
    problems = []
    tools = {t.name: t for t in sl.load_tools()}
    root = sl.SCHEMAS / "examples"
    for tool_dir in sorted(p for p in root.glob("*") if p.is_dir()):
        tool = tools.get(tool_dir.name)
        if tool is None:
            problems.append(f"examples/{tool_dir.name}: no such tool")
            continue
        v = Draft202012Validator(sl.inline_refs(tool.doc["inputSchema"], defs))
        for kind in ("valid", "invalid"):
            for ex in sorted((tool_dir / kind).glob("*.json")):
                ok = v.is_valid(sl.load_json(ex))
                if (kind == "valid") != ok:
                    problems.append(f"{ex.relative_to(sl.REPO)}: expected {kind}, got {'valid' if ok else 'invalid'}")
    return problems


def main() -> int:
    problems = check()
    for p in problems:
        print(p, file=sys.stderr)
    tools = sl.load_tools()
    if not problems:
        print(f"schemas ok: {len(tools)} tools, {len(sl.load_errors())} error codes, {len(sl.load_defs())} defs")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
