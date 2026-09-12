# Bootstrap SQL

Applied in lexical order by `just db-reset` against `DATUM_BOOTSTRAP_URL` as the
bootstrap superuser. Every file is idempotent: a second run is a no-op and
produces no errors.

| File | Purpose |
|---|---|
| `00-init-roles.sql` | Five roles from D3 §1.1 / CONTRACT §8a. Dev-only password `datum` on the two LOGIN roles. |
| `01-database.sql` | Creates `datum_test_template` and `datum_test`, owned by `datum_owner`. Marks the template. |
| `02-grants.sql` | Schema classes `app` / `transient` / `audit` and default privileges (D-W1-2), inside both databases. |

## MacBook (Homebrew postgresql@17)

The server is `127.0.0.1:5432` (never `localhost`). The login user is a superuser
with `trust` on loopback. Canonical client: `$(brew --prefix postgresql@17)/bin`;
`/opt/homebrew/bin/psql` also resolves.

```sh
psql -h 127.0.0.1 -d postgres -f dev/sql/00-init-roles.sql
psql -h 127.0.0.1 -d postgres -f dev/sql/01-database.sql
psql -h 127.0.0.1 -d postgres -f dev/sql/02-grants.sql
```

## Docker (`dev/compose.yml`)

`dev/compose.yml` is the `postgres:17` image (owned by the workspace skeleton).
Bring the service up, then apply the same files against `DATUM_BOOTSTRAP_URL`:

```sh
docker compose -f dev/compose.yml up -d
psql "$DATUM_BOOTSTRAP_URL" -f dev/sql/00-init-roles.sql
psql "$DATUM_BOOTSTRAP_URL" -f dev/sql/01-database.sql
psql "$DATUM_BOOTSTRAP_URL" -f dev/sql/02-grants.sql
```
