# datum-db

Kernel persistence: write/read pools, the sealed `Tx` that binds every `datum.*`
setting, the multi-crate migration runner, and catalogue DDL lints.

## Connection URLs

Three URLs, never derived from each other:

| Variable | Role | Used for |
|---|---|---|
| `DATUM_DATABASE_URL` | `datum_app` | application writes and reads |
| `DATUM_MIGRATE_DATABASE_URL` | `datum_migrate` | migrations and catalogue work |
| `DATUM_BOOTSTRAP_URL` | a superuser | **only** `CREATE`/`DROP DATABASE` and role administration |

Role connections are built from `DATUM_DATABASE_URL` / `DATUM_MIGRATE_DATABASE_URL`
(or from `datum-test`'s `app_pool()` / `migrate_pool()`). They are never produced
by rewriting `DATUM_BOOTSTRAP_URL` (user swap). That rewrite is what broke Linux
CI: a unix-socket bootstrap or a password-authenticated superuser yielded `28000`
peer-authentication or `28P01` password failures on the swapped URL.

`sqlx` does not default a missing user to the OS account the way `libpq` does.
If the bootstrap URL has no userinfo, tests fill in `$USER` / `$LOGNAME`.

### TCP (password superuser)

Use this when the bootstrap user is password-authenticated and the password
**differs** from the `datum_app` / `datum_migrate` role passwords:

```
postgres://<superuser>:<pw>@127.0.0.1:5432/postgres?sslmode=disable
```

Example (throwaway superuser; drop it afterwards):

```
DATUM_BOOTSTRAP_URL=postgres://datum_bootstrap:not-the-role-password@127.0.0.1:5432/postgres?sslmode=disable
```

### Percent-encoded unix socket (Linux CI)

`sqlx` / `libpq` treat a percent-encoded host that decodes to a path as a unix
socket **directory**. The socket file is `<dir>/.s.PGSQL.<port>` (port defaults
to 5432). Encode each `/` in the directory as `%2F`.

Debian / Ubuntu (`postgresql` cluster, directory `/var/run/postgresql`):

```
postgres://<superuser>:<pw>@%2Fvar%2Frun%2Fpostgresql/postgres?sslmode=disable
```

That is:

- userinfo: `<superuser>:<pw>`
- host: `%2Fvar%2Frun%2Fpostgresql` → `/var/run/postgresql`
- database: `postgres`

Homebrew on macOS (directory `/tmp`):

```
postgres://<superuser>:<pw>@%2Ftmp/postgres?sslmode=disable
```

A query-parameter form (`postgres://<superuser>:<pw>@/postgres?host=/var/run/postgresql`)
is accepted by libpq; this crate documents and tests the **percent-encoded host**
form above, which is what Linux CI must use.

Do not put the socket path in the URL un-encoded (`host=/var/run/postgresql` as
the host field). The parser would split on `/`.
