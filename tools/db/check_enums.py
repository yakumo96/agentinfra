"""The CHECK (... IN (...)) lists in migrations must equal the enums in schemas/common/defs.json."""
from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "schema"))
import schemalib as sl  # noqa: E402

# (table, column) -> defs enum name
BINDINGS = {
    ("runs", "state"): "RunState",
    ("work_items", "state"): "WorkItemState",
    ("proposals", "state"): "ProposalState",
    ("workspaces", "state"): "WorkspaceState",
    ("workspaces", "consistency"): "Consistency",
    ("commits", "consistency"): "Consistency",
    ("actors", "role"): "Role",
    ("tokens", "role"): "Role",
}


def table_bodies(sql: str) -> dict[str, str]:
    return {m.group(1): m.group(2) for m in re.finditer(r"CREATE TABLE (\w+) \((.*?)\n\);", sql, re.S)}


def main() -> int:
    sql = "\n".join(p.read_text() for p in sorted((sl.REPO / "migrations").glob("[0-9]*.sql")))
    tables = table_bodies(sql)
    defs = sl.load_defs()
    problems = []
    for (table, column), enum in BINDINGS.items():
        body = tables.get(table)
        if body is None:
            problems.append(f"{table}: table not found")
            continue
        m = re.search(rf"\b{column}\b[^\n]*?(?:\n\s+)?CHECK \({column} IN\s*\(?\s*(.*?)\)\)", body, re.S)
        if not m:
            problems.append(f"{table}.{column}: no CHECK ... IN list")
            continue
        values = re.findall(r"'([^']*)'", m.group(1))
        if values != defs[enum]["enum"]:
            problems.append(f"{table}.{column}: {values} != defs.{enum} {defs[enum]['enum']}")
    for p in problems:
        print(p, file=sys.stderr)
    if not problems:
        print(f"migration enums ok: {len(BINDINGS)} columns")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
