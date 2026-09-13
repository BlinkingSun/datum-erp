-- Drop the one-shot audit.redact registrar (neutralizes 0001 for lint-sql-migrations).

DROP FUNCTION IF EXISTS esign._register_hash_redact();
