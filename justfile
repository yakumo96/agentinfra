# Single entry point for checks and tests. Recipes only use tools that work offline
# unless noted; absorbed LayerFS crates need crates.io (see Cargo.toml).
# 检查和测试的唯一入口。除非另有说明，配方只用离线可用的工具；吸收的 LayerFS crate 需要 crates.io（见 Cargo.toml）。

set shell := ["bash", "-euo", "pipefail", "-c"]

default: check test

# schema validity, generated code freshness, manifests, types
# schema 有效性、生成代码是否最新、清单、类型
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
# 重新执行初次吸收（会覆盖吸收的 crate；只用于从头开始）
absorb-layerfs:
    python3 -I tools/upstream/absorb.py init layerfs --force

# Upstream change between two mirror commits, translated to our paths and crate names
# 两个镜像提交之间的上游变更，翻译成我们的路径和 crate 名
upstream-diff project from to:
    python3 -I tools/upstream/absorb.py diff {{project}} {{from}} {{to}}
