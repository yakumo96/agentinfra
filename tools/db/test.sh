#!/usr/bin/env bash
# Run every migration on a throwaway PostgreSQL cluster, then tests/sql/*.sql
# (each in a fresh database) and tests/sql/*.sh (concurrency tests).
#
#   tools/db/test.sh            # needs initdb/postgres (PostgreSQL 16) on PATH or PG_BIN
#
# 在临时 PostgreSQL 集群上执行全部迁移，然后运行 tests/sql/*.sql（每个用新数据库）
# 和 tests/sql/*.sh（并发测试）。需要 PATH 或 PG_BIN 里有 initdb/postgres（PostgreSQL 16）。
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
PG_BIN="${PG_BIN:-$(dirname "$(command -v initdb 2>/dev/null || echo /usr/lib/postgresql/16/bin/initdb)")}"
RUN_AS=()
if [ "$(id -u)" = 0 ]; then RUN_AS=(runuser -u nobody --); fi

WORK="$(mktemp -d "${TMPDIR:-/tmp}/agentinfra-pg.XXXXXX")"
chmod 755 "$WORK"
[ "${#RUN_AS[@]}" -gt 0 ] && chown nobody "$WORK"
export PGHOST="$WORK" PGPORT=5499 PGUSER="$("${RUN_AS[@]}" id -un)"

cleanup() {
  "${RUN_AS[@]}" "$PG_BIN/pg_ctl" -D "$WORK/data" -m immediate stop >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

"${RUN_AS[@]}" "$PG_BIN/initdb" -D "$WORK/data" -A trust -E UTF8 --no-locale >/dev/null
"${RUN_AS[@]}" "$PG_BIN/pg_ctl" -D "$WORK/data" -l "$WORK/log" -w \
  -o "-c listen_addresses='' -k $WORK -p $PGPORT -c fsync=off" start >/dev/null

PSQL=(psql -X -q -v ON_ERROR_STOP=1 --no-psqlrc)

fresh_db() {
  local db="$1"
  createdb "$db"
  for m in "$REPO"/migrations/[0-9]*.sql; do
    "${PSQL[@]}" -d "$db" -f "$m" >/dev/null
  done
}

pass=0
for t in "$REPO"/tests/sql/*.sql; do
  [ "$(basename "$t")" = fixtures.sql ] && continue
  name="$(basename "$t" .sql)"
  db="t_${name//-/_}"
  fresh_db "$db"
  "${PSQL[@]}" -d "$db" -f "$REPO/tests/sql/fixtures.sql" >/dev/null
  if "${PSQL[@]}" -d "$db" -f "$t" >"$WORK/$name.out" 2>&1; then
    echo "ok   $name"; pass=$((pass + 1))
  else
    echo "FAIL $name"; cat "$WORK/$name.out"; exit 1
  fi
done
for t in "$REPO"/tests/sql/*.sh; do
  name="$(basename "$t" .sh)"
  db="t_${name//-/_}"
  fresh_db "$db"
  "${PSQL[@]}" -d "$db" -f "$REPO/tests/sql/fixtures.sql" >/dev/null
  if PGDATABASE="$db" bash "$t" >"$WORK/$name.out" 2>&1; then
    echo "ok   $name"; pass=$((pass + 1))
  else
    echo "FAIL $name"; cat "$WORK/$name.out"; exit 1
  fi
done
echo "sql tests: $pass passed"
