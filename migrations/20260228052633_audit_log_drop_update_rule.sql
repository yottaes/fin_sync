-- DROP UPDATE rule so ON CONFLICT DO NOTHING works for dedup.
-- Append-only semantics enforced by application (no UPDATE queries issued).
-- DELETE rule stays — audit rows are never deleted.
DROP RULE no_update_audit ON audit_log;
