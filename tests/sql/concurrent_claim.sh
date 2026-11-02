# 12 claimers race for 60 items: every item is claimed exactly once.
set -euo pipefail
PSQL=(psql -X -q -t -A -v ON_ERROR_STOP=1 --no-psqlrc)
"${PSQL[@]}" -c "INSERT INTO infra.work_items (id, tenant_id, repo_id, title, spec, acceptance, priority, created_by)
  SELECT uid('wi', g), 't1', 'demo', 'item ' || g, 's', 'a', g % 7, 'act_00000000000000000000000001'
  FROM generate_series(1, 60) g;"
for w in $(seq 1 12); do
  ( for i in 1 2 3 4 5 6; do
      "${PSQL[@]}" -c "SELECT (infra.claim_work_item('t1', 'demo', 'act_0000000000000000000000000$((w % 2 + 2))')).id" >> "$PGHOST/claims.$w"
    done ) &
done
wait
total=$(cat "$PGHOST"/claims.* | grep -c '^wi_' || true)
unique=$(cat "$PGHOST"/claims.* | grep '^wi_' | sort -u | wc -l)
claimed=$("${PSQL[@]}" -c "SELECT count(*) FROM infra.work_items WHERE state = 'claimed'")
leases=$("${PSQL[@]}" -c "SELECT count(*) FROM infra.leases WHERE resource_kind = 'work_item_claim' AND state = 'active'")
echo "claims=$total unique=$unique claimed=$claimed leases=$leases"
[ "$total" = 60 ] && [ "$unique" = 60 ] && [ "$claimed" = 60 ] && [ "$leases" = 60 ]
