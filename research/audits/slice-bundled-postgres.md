# Sweep: bundled PostgreSQL (ADR 0003) — plan-audit slice 7

Slice: BUNDLED POSTGRESQL on macOS, Windows, Linux. Windows is the hostile case.
Author: adversarial researcher. Date: 2026-09-11.
Sources: `docs/adr/0003-database.md`, `docs/adr/0004-append-only-ledger.md`, `docs/adr/0005-compliance-in-kernel.md`, `docs/02-architecture.md` §§6–7, `docs/01-vision-and-scope.md` success criterion 1, `PLAN.md` (waves + §10).
Product files were not edited.

---

## Verdict

**Not realistic for v1.** Shipping and lifecycle-managing a PostgreSQL cluster inside a desktop installer on all three platforms, silently, without a DBA, without Docker, and in under ten minutes, is not a Wave 1–3 foundation task. It is a packaging product of its own.

Keep **PostgreSQL as the only dialect** (ADR 0003 first sentence). **Defer bundling.** Make the escape hatch the v1 product: the installer (or first-run wizard) asks for a connection string, or documents a one-page “install Postgres, then Wicket” path.

This is an **ADR 0003 revisit**. The ADR already names the cost (“installer complexity, which is genuine work”) and already lists the revisit trigger (“bundled-cluster upgrade path proves genuinely unreliable”). The honest move is to fire that trigger **before** writing the sidecar, not after a shop loses a ledger to a half-finished `pg_upgrade`.

`docs/01-vision-and-scope.md` §8 still lists “Whether to bundle a database or require one” as an open question. ADR 0003 pretended it was closed. Those two documents disagree. Close the disagreement in favor of the escape hatch for v1.

---

## What the documents actually claim

| Claim | Where | Tension |
|---|---|---|
| Ten-minute install, no DBA, no container runtime, three OSes | vision §7 success criterion 1 | Bundled cluster is the only way this is literally true |
| PostgreSQL only, no dialect abstraction | ADR 0003 | Keep this. It is cheap and correct |
| Bundled with the desktop installer; user never learns it is there | ADR 0003 Decision | Conflicts with every Windows fact below |
| Escape hatch for shops that already run Postgres | ADR 0003 | Already designed. Unused if bundling is mandatory for v1 |
| Deferrable constraints, exclusion constraints, grants, PITR, logical replication | ADR 0003 Consequences | These require **real** Postgres, not PGlite |
| Application role has INSERT/SELECT on audit, not UPDATE/DELETE | ADR 0005, PLAN invariant 3 | Grants are theater if the same Windows user owns the cluster and can `psql` as superuser |
| Typical topology: one office machine, LAN browsers and tablets, no internet | architecture §6 | Postgres can stay on `127.0.0.1`; **Wicket** must bind the LAN → Windows Firewall still fires |
| Wave 1 workspace owns `justfile`, `.github/workflows/`, `dev/` | PLAN §3 | No Postgres-for-tests strategy |
| Wave 3: `wicket-server`, modules, web shell, Tauri desktop shell | PLAN §3 | Bundling is **implied**, not named |
| Out of scope: Phase 2 modules, GL, multi-tenancy, runtime plugins | PLAN §10 | **Bundling is not listed.** It is therefore accidentally in-scope via Wave 3 Tauri, with zero lanes, zero acceptance, zero OS split |

PLAN is silent on how `cargo test` gets a Postgres before any installer exists. That is a missing Wave 1 `workspace` subtask, not a Wave 3 packaging surprise.

---

## Realistic / not for v1

| Platform | Bundle in v1? | Why |
|---|---|---|
| Linux | Possible later; not needed for ten minutes | `apt`/`dnf` Postgres is a two-minute documented step. systemd unit is the native lifecycle. Bundling reinvents the distro package |
| macOS | Possible later as a Postgres.app clone | Postgres.app proves the UX. It is a **developer menubar app**, not a 30-person shop server, and it shuts down when the app quits |
| Windows | **No** | See ranked breakages. Closest prior art (Odoo all-in-one) **requires admin**, registers a Windows Service, and **discourages Windows for production** |

A 30-person medical-device shop on a spare office PC is the **Windows** case. macOS-and-Linux-first bundling does not pass success criterion 1 as written.

The ten-minute test is adversarial on purpose:

1. Double-click Wicket.msi / .exe on a stock Windows 10/11 shop PC.
2. No admin / UAC (vision: “no database administrator”; shops lock local admin).
3. No Docker Desktop (vision: “no container runtime”).
4. Existing Postgres on 5432 from an old Odoo / EDB / Docker leftover is allowed by reality, not by the spec.
5. Defender is on. Firewall is on. Fast Startup / Fast User Switching are on.
6. Someone logs off at 17:00. Tablets on the floor must keep scanning.
7. Power blip. WAL replay. App comes back without a DBA.
8. Six months later, Wicket ships a Postgres major bump. Unattended `pg_upgrade`. Both binaries present.

Steps 2, 6, 7, and 8 fail if Postgres is a user process. Steps 2 and 6 fail if it is a Windows Service (admin to register). Step 4 fails unless the installer owns port selection. Step 3 is the entire point. **The test is not passable on Windows in v1 without lying about “silent” or “no admin.”**

Honest v1 ten-minute story: “Postgres is already running, or you install EDB/Postgres.app/apt first (5–15 min, possibly with admin), then Wicket connects in under two.” That **fails** success criterion 1 as written. Amend the criterion rather than the physics.

---

## Windows-specific breakages, ranked

Severity: **P0** = cannot ship a silent no-admin installer. **P1** = field-loss / compliance. **P2** = first-run friction that blows the ten-minute clock. **P3** = packaging tax.

### P0-1. User process vs Windows Service (logoff kills the shop)

`pg_ctl start` as the logged-in user ties the postmaster to the interactive session. Logging off that user tears the process tree down. Shop tablets talking to that machine go dark.

The documented Windows way to survive logoff is `pg_ctl register` — a **system service** (`https://www.postgresql.org/docs/current/app-pg-ctl.html`). Creating a service requires administrator rights. Service logon account, password, and `ProgramData` ACLs are admin work. Session 0 isolation (Vista+) means the service has no desktop; that is fine for a daemon and fatal if anyone expected a GUI “Postgres is running” toast from the service.

Odoo’s all-in-one installer chose the service path and **requires UAC**. Their own docs say Windows packaging is “for testing or running single-user local instances” and **production on Windows is discouraged** (`https://www.odoo.com/documentation/saas-18.2/administration/on_premise/packages.html`). That is the most successful ERP-adjacent analog, and they flinched.

If Wicket stays a user process so it can skip admin: the first person to log off the office PC stops production. Fast User Switching is not a logoff but still isolates the session. A dedicated “always logged in” kiosk account is an SOP, not an installer feature, and IT will not bless it at a regulated shop.

**Cite:** pg_ctl register is Windows-only and creates a system service; BUG #6201 (logoff can crash backends even when Postgres **is** a service: `https://www.postgresql.org/message-id/201109092059.p89KxoVr078697@wwwmaster.postgresql.org`); Session 0 isolation (`https://kb.firedaemon.com/support/solutions/articles/4000086228-microsoft-windows-session-0-isolation-and-interactive-services-detection`).

### P0-2. Firewall first-run prompt vs LAN topology

Architecture §6: office machine serves browsers and tablets on the LAN. Postgres itself can listen only on `127.0.0.1` (Wicket proxies). That avoids a **postgres.exe** firewall dialog. It does **not** avoid a **wicket-server.exe** prompt the first time the Rust binary binds `0.0.0.0`.

Windows Defender Firewall’s “Windows Firewall has blocked some features of this app” is an interactive dialog. Adding a rule silently needs admin (`New-NetFirewallRule`). A ten-minute **silent** install cannot click Allow.

If someone naively sets `listen_addresses = '*'` on the bundled cluster so tablets can hit Postgres directly, they get a second prompt, an exposed superuser, and a compliance finding. Do not do that. Keep Postgres loopback-only. Still: Wicket’s LAN bind is the prompt.

**Cite:** EDB/Postgres Pro installers treat firewall as an installer checkbox, not a silent default (`https://postgrespro.com/docs/enterprise/9.6/binary-installation-on-windows`). Remote-access guides require an explicit inbound rule (`https://postgre-sql.github.io/postgresql-allow-remote-connections.html`).

### P0-3. No production desktop app does this without admin and without Docker

See prior-art table. The empty cell is the finding. Odoo, EDB, Bitnami, Coverity: admin + service. GitLab Omnibus: Linux package, not desktop, not Windows. Everything in the “modern open source” set (Supabase, PostHog, Outline, Plane, Twenty, Mattermost) cheats with Docker or an external Postgres. Postgres.app is macOS. ElectricSQL’s Tauri+Postgres demo is an experiment (`https://github.com/electric-sql/electric-tauri-postgres`, 23 stars).

**Do not staff a Wave 3 lane as if this were a solved Tauri sidecar.** The sidecar pattern exists. Lifecycle-managing a **cluster** (initdb, port, WAL, upgrade, AV, service) is not the sidecar pattern.

### P1-1. Antivirus / Defender locking WAL or data files

Microsoft Defender has quarantined PostgreSQL WAL as malware (false positive `Exploit:HTML/IframeRef`) and **terminated the service** (`http://notesofaprogrammer.blogspot.com/2017/06/windows-defender-interferes-with.html`). pgsql-hackers: “Microsoft defender locks the data files in the data directory during the scans” (`https://www.postgresql.org/message-id/CAOEeMcUz6ar1wTVBpnOkugycBbta03GvNk4F5VguiDX8TYkOrw@mail.gmail.com`).

Exclusions require admin (`Add-MpPreference -ExclusionPath ...`). Tamper Protection can block them. A silent per-user installer cannot add a machine-wide exclusion. Shop PCs with CrowdStrike / SentinelOne / Sophos are worse; those products treat a bundled `postgres.exe` writing WAL as exactly the behavior they hunt.

ADR 0003 already admits “some antivirus and endpoint products dislike a bundled server process and will need documentation to appease.” Documentation is not a ten-minute silent install. It is a ticket to IT.

### P1-2. Unclean shutdown / WAL replay on a shop PC

Postgres is designed to recover from `immediate` shutdown and power loss **if fsync actually hit the disk**. Shop PCs: consumer SSDs with DRAM-less controllers, Windows write cache, “Fast Startup” (hybrid shutdown that is not a real shutdown), Defender holding a WAL file, UPS optional.

`pg_ctl stop -m immediate` “will lead to a crash-recovery cycle during the next server start” (official pg_ctl docs). That is fine when a DBA watches the log. Unattended, a failed recovery (`pre-existing shared memory block is still in use` — BUG #6201) leaves Wicket showing a connection error to a machinist with gloves on.

WAL replay is a reason to pick Postgres, not a reason to pretend the installer owns it. An external Postgres running as a service is the same physics with twenty years of ops lore. A bundled cluster has to **reimplement** that lore inside Wicket.

### P1-3. Major-version upgrade: both binaries must ship

`pg_upgrade` **requires old and new bindirs and datadirs**. Official docs: “Always run the pg_upgrade binary of the new server, not the old one.” Windows: “you must be logged into an administrative account” and quote paths with spaces (`https://www.postgresql.org/docs/current/pgupgrade.html`). It needs write permission in the current working directory (`pg_upgrade_internal.log` failures are a Windows classic: `https://stackoverflow.com/questions/34664236/pg-upgrade-on-windows-cannot-write-to-log-file-pg-upgrade-internal-log`).

So every major bump of a bundled Wicket must:

1. Ship both PostgreSQL N and N+1 (~hundreds of MB extra).
2. Stop the cluster (production down).
3. `initdb` the new datadir.
4. Run `pg_upgrade` unattended as a non-admin user (docs say admin).
5. Switch ports/data pointers.
6. On failure, roll back without a DBA.

GitLab Omnibus has a packaging team and a documented yearly cadence and still **aborts package upgrades** if Postgres was not upgraded (`https://docs.gitlab.com/18.8/administration/package_information/postgresql_versions/`). PostHog hobby compose pinned Postgres 12 “until we have a process for pg_upgrade” (`docker-compose.hobby.yml` comment, historical). ADR 0003’s own revisit trigger is this path. Treat it as already pulled.

Logical dump/restore is the fallback and is slower and still needs both servers. PGlite cannot `pg_upgrade` at all.

### P1-4. Grant-level audit append-only vs OS-owner superuser (slice 2)

PLAN invariant 3 / ADR 0005: application role has INSERT+SELECT on audit, not UPDATE/DELETE. That is a **defense against application bugs**. It is not a defense against the Windows user who owns `PGDATA`.

That user can:

- Read the superuser password from Wicket’s config / credential file in their own `%AppData%`.
- Flip `pg_hba.conf` to `trust` and restart.
- Run `psql -U postgres` and `UPDATE`/`DELETE` the audit table.
- Copy the data directory off the box.

21 CFR 11 physical-access control is an SOP (locked office PC, Windows login, backup media). Database GRANTs do not replace that. **Bundling makes the OS user and the cluster owner the same person by construction**, which is the worst possible topology for pretending grants are a compliance boundary.

Escape-hatch / external Postgres can run as a dedicated `postgres` Windows service account that shop operators are not. That is the only layout in which GRANT-level append-only is more than an app-bug net. Slice 2 should not claim more.

### P2-1. Port 5432 collision, named-pipe vs TCP, `::1` vs `127.0.0.1`

Default port 5432 collides with EDB, Odoo’s bundled `PostgreSQL_For_Odoo`, Docker Desktop publishes, and leftover installs. A silent installer must probe, pick an ephemeral or high port, and persist it. `postgresql_embedded` already does ephemeral ports for tests; a product cluster should too.

Windows has no Unix-socket tradition. Recent Postgres can use AF_UNIX on Windows 10 1803+, with path-length limits. Default is TCP. Named pipes are not the Postgres-on-Windows default.

`localhost` on Windows is dual-stack and **IPv6-first**. If `listen_addresses` is `127.0.0.1` only, clients connecting to `localhost` try `::1` first: connection refused or a ~10s fallback (`https://dev.to/skucherenko/the-localhost-trap-a-10-second-database-connection-on-windows-3le7`; Craig Ringer on `::1`: `https://stackoverflow.com/questions/17648677/is-the-server-running-on-host-localhost-1-and-accepting-tcp-ip-connections`). Shop-floor 500ms scan budget dies on a 10s connect. **Wicket must connect to `127.0.0.1`, never `localhost`, and listen on both or only v4 explicitly.**

### P2-2. VC++ redistributable, ICU, codepage vs UTF8

EDB’s Windows installer installs VC++ runtimes by default. Unattended with `install_runtimes=0` fails on a machine without `msvcp140.dll` (`https://github.com/EnterpriseDB/edb-installers/issues/155`). zonkyio/embedded-postgres historically required **VC++ 2013** specifically (`https://github.com/zonkyio/embedded-postgres` README: “Running tests on Windows does not work” without that redist).

A Tauri sidecar that shells out to `postgres.exe` built with MSVC will fail identically unless Wicket ships or prerequisites the matching redist. Installing a redist is often another UAC.

**UTF8 is not the unconditional Windows default.** `initdb` default locale provider is still **libc**; encoding is derived from the Windows locale (e.g. `English_United States.1252` → WIN1252). ICU provider defaults encoding to UTF8, but libc settings are still initialized (`https://www.postgresql.org/docs/15/app-initdb.html`). On Windows, UTF8 **can** be used with any libc locale (`https://www.postgresql.org/docs/15/multibyte.html`). Wicket must pass `-E UTF8` (and a chosen locale) on every `initdb`, not trust the shop PC’s codepage. Collation differences across shop PCs will otherwise make tests and backups non-portable.

### P2-3. Restricted token: Postgres refuses to run as Administrator

`initdb`/`postgres` on Windows re-exec with a restricted token that drops Administrators. Data dirs whose ACLs are “Administrators only” (including some `mkdtemp` trees, and often `Program Files`) then fail with permission denied (`https://stackoverflow.com/questions/30936467/executing-batch-file-for-postgre-dbinit-with-nsis-gives-permission-denied`; Coverity: “Execution of PostgreSQL by a user with administrative permissions is not permitted”). Software Restriction Policies yield error 1260.

If the Wicket installer is elevated (to create a service / firewall rule), `initdb` as that token **fails** unless it creates the datadir and ACLs **before** dropping privileges, as a dedicated service account. This is the entire EDB installer. It is not a sidecar one-liner.

Python `embedded-postgres` 18.6 documents the same trap on Windows Administrator accounts (`https://pypi.org/project/embedded-postgres/18.6.2/`).

### P2-4. Path length, spaces, AppData vs ProgramData, multiple users

`MAX_PATH` is 260 unless the process and OS opt into long paths (`https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation`). Postgres relation files under a deep `%LOCALAPPDATA%\Wicket ERP\postgresql\data\...` plus a long database name will hit it. EDB default `C:\Program Files\PostgreSQL\N\data` works only because it is short **and** the installer is admin.

Spaces in `Program Files` and `Application Data` require quoting everywhere. Trailing backslash inside quotes is a known `pg_ctl` footgun (`https://www.postgresql.org/message-id/200410271716.i9RHGXg22084@candle.pha.pa.us`).

| Location | Admin? | Survives logoff? | Shared across local users? |
|---|---|---|---|
| `%LOCALAPPDATA%` | no | no (user process) | no — second Windows login gets a second cluster or access denied |
| `%PROGRAMDATA%` | write usually needs admin | if service | yes |
| `Program Files` | yes | if service | yes; `initdb` hates the ACLs |

A 30-person shop with two office logins on one PC cannot store the regulated ledger in `%LOCALAPPDATA%` of user A.

### P3. Installed size, three packagings, Tauri sidecar is the easy 5%

ADR 0003: “a few hundred megabytes.” Postgres.app single-version ~120MB download, multi-version ~500MB (`https://postgresapp.com/downloads.html`). Tauri `externalBin` sidecar is per-target-triple (`https://v2.tauri.app/develop/sidecar/`). That ships `postgres.exe`. It does not ship initdb policy, service, upgrade, or AV docs.

`postgresql_embedded` (theseus-rs) can `bundled` the archive into the Rust binary and unpack at runtime to `%USERPROFILE%\.theseus\postgresql`. That is a **dev/test** lifecycle, not a regulated shop cluster: cache location is per-user, upgrades are “download latest,” no `pg_upgrade` story, no service.

---

## Prior art table

### Tools that bundle or embed Postgres binaries

| Name | Link | What it actually is | Windows? | Admin? | Lifecycle | Production desktop? |
|---|---|---|---|---|---|---|
| Postgres.app | https://postgresapp.com/ · https://github.com/PostgresApp/PostgresApp | Native macOS app; binaries in `Contents/Versions`; datadir in `~/Library/Application Support/Postgres` | no | no | Start/stop with the app; quit ⇒ server stops | Developer machine, not a shop server |
| EDB installer | https://www.enterprisedb.com/docs/supported-open-source/postgresql/installing/windows/ | Official Windows/macOS installer; zip “for users who wish to include Postgres as part of another application installer” | yes | **yes** | Windows Service, VC++ redist, Stack Builder | Server install, not silent sidecar |
| BigSQL / OpenSCG | historical | Binary distro; project faded | was | varies | Dead as a strategy | no |
| PostgreSQL portable / zip binaries | https://www.postgresql.org/download/windows/ (EDB zip); community “portable” trees | Unzip and `pg_ctl`; you own everything | yes | no if not a service | You write the lifecycle | Dev/trial (e.g. https://github.com/kanad13/postgresql-windows-no-admin — **not intended for production**) |
| zonkyio/embedded-postgres | https://github.com/zonkyio/embedded-postgres | Java test helper; unpacks zonky binaries to temp | yes, with VC++ landmines | no | JVM lifetime / shutdown hook | **tests only** |
| io.zonky.test.embedded-postgres | Maven artifact for the above | same | yes | no | tests | tests |
| zonkyio/embedded-postgres-binaries | https://github.com/zonkyio/embedded-postgres-binaries | Reduced-size PG builds consumed by zonky, Go, Rust | yes | n/a | n/a | binary feed |
| fergusstrange/embedded-postgres | https://github.com/fergusstrange/embedded-postgres | Go test helper; downloads binaries | yes | no | process lifetime | tests |
| theseus-rs/postgresql-embedded · crates.io `postgresql_embedded` | https://github.com/theseus-rs/postgresql-embedded · https://crates.io/crates/postgresql_embedded | Rust: download or `bundled` archive; `setup/start/stop`; cache `%USERPROFILE%\.theseus\postgresql` | yes | no | process; ephemeral ports; **not a Windows Service** | closest Rust primitive; still not a shop cluster |
| pg_embed (older Rust) | various | predecessor shape | yes | no | process | abandoned-ish; ElectricSQL demo used it |
| Python embedded-postgres | https://pypi.org/project/embedded-postgres/ | wheels with PG 18.6; documents Windows Administrator ACL trap | yes | no (admin **breaks** it) | process | tests / local |
| Tauri sidecar | https://v2.tauri.app/develop/sidecar/ | Pack extra binaries per target triple; `shell().sidecar()` | yes | no | **you** spawn/kill | pattern, not Postgres |
| ElectricSQL Tauri+Postgres | https://electric.ax/blog/2024/02/05/local-first-ai-with-tauri-postgres-pgvector-llama · https://github.com/electric-sql/electric-tauri-postgres | Experiment: zonky/EDB binaries + pg_embed inside Tauri | claimed | no | demo | **not production** |

### Products: what they actually ship

| Product | Link | What they ship for Postgres | Docker? | Desktop silent Windows sidecar without admin? |
|---|---|---|---|---|
| GitLab Omnibus | https://docs.gitlab.com/omnibus/settings/database/ | Bundled PG inside the **Linux** package; Chef-managed; external PG escape hatch | no (native Linux pkg) | **no** — GitLab does not run on Windows (`https://docs.gitlab.com/install/requirements/`) |
| Mattermost | https://docs.mattermost.com/deployment-guide/server/linux/deploy-ubuntu.html | **Requires** PostgreSQL 14+ as a separate install or compose service | optional | no |
| Supabase local / self-host | https://supabase.com/docs/guides/self-hosting · https://github.com/supabase/supabase/tree/master/docker | CLI and self-host are Docker Compose. “The fastest and recommended way to self-host Supabase is to use Docker.” | **yes** | no |
| PostHog | https://github.com/PostHog/posthog/blob/master/docker-compose.hobby.yml | Hobby = compose with `postgres:15.12-alpine` (+ ClickHouse, Kafka, …). Historical pin to PG12 “until we have a process for pg_upgrade” | **yes** | no |
| Outline | typical compose / managed PG | managed or Docker Postgres | yes | no |
| Plane | compose / AIO | Docker Postgres | yes | no |
| Twenty | docker-compose; discussion #5449 | Docker `db` service; people rip it out to point at Supabase | **yes** | no |
| Odoo all-in-one Windows | https://www.odoo.com/documentation/saas-18.2/administration/on_premise/packages.html · NSIS `setup.nsi` | Embeds EDB installer `--mode unattended`, service `PostgreSQL_For_Odoo`, service account `openpgsvc` | no | **admin required**; production on Windows discouraged |
| Metabase (analog, not PG-bundled) | product docs | H2 for easy start, **real DB for production** | n/a | The honest ten-minute cheat. Wicket already rejected SQLite/H2 on capability |
| Coverity Connect | Black Duck article | “Embedded database” = bundled PG as a service | no | admin; still hits restricted-token / 1260 |

**Named production desktop app that silently lifecycle-manages Postgres on Windows without admin and without Docker:** none found. If one exists it is obscure. Odoo is the existence proof that “bundle on Windows” means **UAC + service**, and even they tell you not to put production there.

---

## PGlite / PG.wasm vs ADR 0004

**Not a serious alternative for the beachhead 30-person shop.** It is a serious **in-process test / demo** engine.

Facts (`https://github.com/electric-sql/pglite/`):

- Real Postgres compiled to WASM, **single-user mode** (no fork). Official: “PGlite is single user/connection.”
- No password authentication. Roles/GRANTs/RLS can be **simulated** by closing and reopening with a `username` (`https://github.com/electric-sql/pglite/issues/401`). The process is still the engine. SET ROLE / reopen-as-superuser is available to anyone who can drive the API.
- `btree_gist` ships, so exclusion constraints are not automatically dead (`https://pglite.dev/extensions/`). Deferrable constraints are core Postgres and should work **inside one session**.
- `CREATE INDEX CONCURRENTLY` has been broken (single-connection). Concurrent shop-floor writers do not exist; they serialize.
- No PITR, no logical replication, no `pg_upgrade` (logical dump only; grants/triggers/RLS often out of scope — `pglite-migrate`).
- Wicket is a **Rust Axum** server. PGlite is a JS/TS library. Embedding it means a JS runtime or a separate WASM build. That is not ADR 0002.

ADR 0003 bought Postgres **because of** deferrable constraints, exclusion/range types, grants, PITR, replication, and concurrent writes. PGlite keeps the SQL dialect and drops the operational properties. Architecture performance: shop-floor scan <500ms with several tablets is a **multi-connection** workload. Serialized WASM is the opposite.

SQLite was already rejected in ADR 0003 on capability. PGlite is SQLite’s shape (in-process file, no daemon) with a Postgres accent. Same rejection, plus WASM operational risk.

Use PGlite (or a WASM PG) only if Wicket later wants a **read-only offline cache** on a tablet — which ADR 0003 already filed as a new decision.

---

## Wave 1–2 test-time Postgres (PLAN is silent)

Kernel crates (`wicket-db`, ledger property tests, migration round-trips) need a real Postgres **before** any installer. This is a **Wave 1 `workspace` subtask** (that lane already owns `justfile`, `.github/workflows/`, `dev/`).

Recommended strategy, in order:

1. **`DATABASE_URL` passthrough.** Developers and CI with an existing cluster just work. SQLx already wants this.
2. **GitHub Actions service container** (`services: postgres:16`) on Linux runners. Cheap, official, matches production dialect. Put it in the workflow the `workspace` lane will write.
3. **Local / Windows shop-PC CI without Docker: `postgresql_embedded` (theseus-rs)** behind a `just test` / `dev/` helper. Ephemeral port, `setup/start/stop`, pin `POSTGRESQL_VERSION`. This is the only option that does not require Docker Desktop on the Windows test node named in the team LOCAL CI FIRST law.
4. **`testcontainers-rs` / `testcontainers-modules` postgres** as an **optional** path when Docker is present. Do not make Docker the default: vision forbids a container runtime for *customers*, and the Windows shop PC may not have it for *developers* either. testcontainers is a CI luxury, not the harness.
5. Pin the major version in one place (`dev/postgres-version` or a workspace env) so tests, CI, and the future bundle cannot drift.

Do **not** use PGlite for `wicket-ledger` property tests. Deferrable constraints and concurrent sessions are the point of those tests.

---

## Is bundling in-scope for THIS build?

**PLAN §10 does not list it.** Wave 3 names a Tauri desktop shell and does not name a Postgres sidecar, cluster init, Windows service, or `pg_upgrade`.

So bundling is currently **in-scope by implication and out-of-scope by effort**. That is how it becomes an unestimated Wave 3 surprise.

**Amendment (recommended):** add to PLAN §10:

> Bundled PostgreSQL lifecycle (initdb, port, service/user-process policy, AV, major-version upgrade) is **out of scope for this build**. v1 uses the ADR 0003 escape hatch. A later packaging wave owns per-OS installers.

And add to Wave 1 `workspace` acceptance:

> `just test` / CI can obtain a Postgres 16 cluster without a product installer: Actions service + `DATABASE_URL` + documented `postgresql_embedded` fallback.

If the project lead refuses the §10 amendment and insists on ten-minute-no-DBA as a Wave 3 gate, **add explicit packaging lanes**, split per OS, and do not start them until kernel + server exist. Windows is not “the Tauri lane also copies postgres.exe.”

---

## PLAN amendment + ADR 0003 revisit (copy-ready)

**ADR 0003 split:**

- **Keep:** PostgreSQL only. No dialect abstraction. Deferrable constraints, exclusion constraints, grants, PITR remain the reason.
- **Defer:** “Bundled with the desktop installer. The user never learns it is there.”
- **Promote to v1:** “With an escape hatch” becomes the default path, not the exception. First-run: connection string, or fail closed with a short “install PostgreSQL, then paste DATABASE_URL” page. Link EDB / Postgres.app / distro packages. Do not shell out to their installers from Wicket in v1.
- **Revisit trigger (already in the ADR):** fire it now, as a planning decision, not after field failure.

**Vision success criterion 1:** rewrite or split:

- v1: “A shop with a running PostgreSQL can install Wicket on three OSes in under ten minutes.”
- Later: “A shop with no DBA and no container runtime can install Wicket+Postgres in under ten minutes on macOS and Linux; Windows requires admin for a service or an external Postgres.”

Do not keep the current sentence and also defer bundling. That is a documented lie.

---

## EXECUTOR / TIER / SPLIT / opus DECISION

| Item | Call |
|---|---|
| **Realistic for v1?** | **No** (Windows). Linux/macOS bundling is later and still not “never learn it is there” for a shop server |
| **TIER** | **opus DECISION** — `DECISION-bundle-pg.md`. Not a grok/cursor typing task. Not a Wave 1–2 build lane |
| **EXECUTOR** | **Do not blind-race a Windows sidecar.** Four incomplete packagers will not converge: service vs user process is a product decision, not an implementation detail. If (and only if) opus says bundle-later and someone still wants a spike, a **read-only spike** on `postgresql_embedded` + Tauri sidecar on **one** OS is enough. Windows sidecar implementation = dedicated packaging wave, senior, with a real Windows shop PC |
| **SPLIT** | **Yes, packaging per OS.** macOS ≈ Postgres.app clone (LaunchAgent). Linux ≈ vendor the distro layout or a systemd user/system unit. Windows ≈ Odoo/EDB (admin + service) **or** refuse. These do not share an installer codebase beyond “here is a tarball of binaries” |
| **opus question** | **bundle-now vs escape-hatch-first** |

Suggested decision options for opus:

1. **Escape-hatch-first (recommended).** Amend ADR 0003 + vision §7 + PLAN §10. Wave 1 gets test Postgres. Wave 3 Tauri does not spawn `postgres`.
2. **Bundle macOS+Linux only, Windows escape-hatch.** Splits the success criterion. Honest about the hostile OS.
3. **Bundle-now including Windows.** Accept admin UAC, Windows Service, Defender exclusion docs, and a `pg_upgrade` project. Schedule as its own wave after the kernel exists. Budget like GitLab/Odoo packaging, not like a sidecar.

Option 3 is how you miss the beachhead ship date. Option 1 is how every serious “we use Postgres” product actually launched (Mattermost, Metabase-with-real-DB, Mattermost, PostHog, Twenty, …).

---

## Cross-links

- Slice 2 (grant-level audit): P1-4 above. Bundled cluster on a user-owned Windows datadir cannot make GRANTs a Part 11 control.
- Architecture §6 LAN topology: keep Postgres on `127.0.0.1`; Firewall still hits Wicket. Connect via `127.0.0.1` not `localhost`.
- PLAN Wave 1 `workspace`: missing test-Postgres subtask. Do not wait for Wave 3.

---

## End
VERDICT: not for v1; escape-hatch-first; ADR 0003 revisit; PLAN §10 must name bundling as out of scope (or add per-OS packaging lanes, not a silent Wave 3 Tauri footnote).
TIER: opus DECISION.
EXECUTOR: no Windows-sidecar blind race.
SPLIT: packaging per OS.
)
