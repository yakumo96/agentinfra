"""Load, check and bundle the schemas/ tree. Standard library plus `jsonschema`/`referencing`."""
from __future__ import annotations

import copy
import json
from dataclasses import dataclass
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SCHEMAS = REPO / "schemas"
DEFS_REF = "../../common/defs.json#/$defs/"


@dataclass
class Tool:
    path: Path
    doc: dict

    @property
    def name(self) -> str:
        return self.doc["name"]


def load_json(path: Path) -> dict:
    with path.open(encoding="utf-8") as f:
        return json.load(f)


def load_defs() -> dict:
    return load_json(SCHEMAS / "common" / "defs.json")["$defs"]


def load_errors() -> list[dict]:
    return load_json(SCHEMAS / "common" / "errors.json")["codes"]


def load_tools() -> list[Tool]:
    tools = [Tool(p, load_json(p)) for p in sorted((SCHEMAS / "tools").glob("*/*.json"))]
    return sorted(tools, key=lambda t: (t.doc["group"], t.name))


def ref_name(ref: str) -> str:
    """'../../common/defs.json#/$defs/X' or '#/$defs/X' -> 'X'."""
    if "#/$defs/" not in ref:
        raise ValueError(f"unsupported $ref {ref!r}")
    return ref.split("#/$defs/", 1)[1]


def inline_refs(schema, defs: dict, seen: tuple[str, ...] = ()):
    """Return a copy of `schema` with every defs $ref replaced by the definition itself."""
    if isinstance(schema, dict):
        if "$ref" in schema:
            name = ref_name(schema["$ref"])
            if name in seen:
                raise ValueError(f"recursive definition {name}")
            rest = {k: v for k, v in schema.items() if k != "$ref"}
            target = inline_refs(copy.deepcopy(defs[name]), defs, seen + (name,))
            if rest:  # sibling keywords such as "default" next to $ref
                return {"allOf": [target], **inline_refs(rest, defs, seen)}
            return target
        return {k: inline_refs(v, defs, seen) for k, v in schema.items()}
    if isinstance(schema, list):
        return [inline_refs(v, defs, seen) for v in schema]
    return schema


def bundle_tool(tool: Tool, defs: dict) -> dict:
    """The MCP-facing form of one tool: self-contained input and output schemas."""
    d = tool.doc
    out = {
        "name": d["name"],
        "group": d["group"],
        "description": d["description"],
        "mutates": d["mutates"],
        "roles": d["roles"],
        "inputSchema": inline_refs(d["inputSchema"], defs),
        "outputSchema": inline_refs(d["outputSchema"], defs),
    }
    if "harness_builtin" in d:
        out["harness_builtin"] = d["harness_builtin"]
    if d.get("long_running"):
        out["long_running"] = True
    return out


def bundle_all() -> dict:
    defs = load_defs()
    return {
        "version": 1,
        "errors": load_errors(),
        "tools": [bundle_tool(t, defs) for t in load_tools()],
    }


def dumps(obj) -> str:
    return json.dumps(obj, ensure_ascii=False, indent=2, sort_keys=False) + "\n"
