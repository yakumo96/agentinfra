# tests

| Directory | What | Language | State |
| --- | --- | --- | --- |
| `sql/` | control-plane functions on a throwaway PostgreSQL (`tools/db/test.sh`) | SQL + shell | running |
| `lib/` | generated Python tool types for test clients | Python | generated |
| `e2e/` | claim → edit → test → submit → publish through MCP and the model API only | TypeScript | M1 |
| `storage/` | content store and FUSE: fsx, fsstress, pjdfstest on a mounted workspace; kill -9 and power-cut simulation | Rust + shell | M1 |
| `fault/` | infrad restart reconciliation, lease expiry, fencing under races | Rust / Python | M1 |
| `security/` | path escape, token scope, container isolation, egress allowlist | Python | M2 |
| `conformance-pi/` | stock pi with overridden built-ins against the same Infra | TypeScript | M2 |

Unit tests live next to their code (`infra/crates/*/tests`, `harness/*/test`, `gateway/tests`).
