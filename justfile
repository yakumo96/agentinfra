# Single entry point for checks and tests. Recipes only use tools that work offline
# unless noted; absorbed LayerFS crates need crates.io (see Cargo.toml).

set shell := ["bash", "-euo", "pipefail", "-c"]

default: check test

# schema validity, generated code freshness, manifests, types
check: check-schemas check-rust check-ts check-py check-absorbed

check-schemas:
    python3 -I tools/schema/validate.py
    python3 -I tools/schema/codegen.py --check
    python3 -I tools/db/check_enums.py

check-rust:
    cargo check --offline --workspace --all-targets
    cargo fmt --all -- --check

check-ts:
    tsc -p harness/tsconfig.json

check-py:
    python3 -m compileall -q gateway/src tests/lib tools

check-absorbed:
    python3 -I tools/upstream/check_absorbed.py

test: test-rust test-ts test-py test-sql

test-rust:
    cargo test --offline --workspace

test-ts:
    node --test harness/*/test/*.test.ts

test-py:
    cd gateway && python3 -m pytest -q

test-sql:
    tools/db/test.sh

codegen:
    python3 -I tools/schema/codegen.py

# Re-run the initial absorption (overwrites absorbed crates; only for a fresh start)
absorb-layerfs:
    python3 -I tools/upstream/absorb.py init layerfs --force

# Upstream change between two mirror commits, translated to our paths and crate names
upstream-diff project from to:
    python3 -I tools/upstream/absorb.py diff {{project}} {{from}} {{to}}
