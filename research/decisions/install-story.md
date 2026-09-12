# DECISION D5 — the install story

Authority: decision authority, team task `erp`.
Date: 2026-09-11.
Status: **decided**. Binding on ADR 0003, `docs/01` §7 criterion 1, `docs/02` §6, and Wave 3 scope.
Inputs: `_team/reports/sweep-plan-bundled-pg.md`, `_team/reports/plan-audit.md` §0.4 / R7 / G9 / G26 /
D5, `_team/reports/spike-regulatory.md` §2.2, `docs/adr/0003-database.md`,
`docs/01-vision-and-scope.md` §7, `docs/02-architecture.md` §6.

---

## 0. The finding that decides this

The adversarial slice is correct and I am confirming its verdict. But it stopped one step short of the
fact that actually settles the argument, and that fact changes what we are trading away.

The slice indicts PostgreSQL's Windows lifecycle: a user-process cluster dies at logoff, a service
needs administrator rights. Both true. **`datum-server` has exactly the same problem.** A Rust binary
launched by the logged-in operator is torn down with that operator's session just as surely as
`postgres.exe` is. A shop floor tablet that comes back at 6am needs the office machine answering with
nobody logged in — and that requires `datum-server` to be a Windows Service whether the database is
bundled, external, SQLite, or a text file.

So the administrator prompt was never a consequence of the database decision. It is a consequence of
wanting a server that survives a logoff, which is the whole product. **Success criterion 1's "no
database administrator" was achievable; its implied "no administrator rights" was not, and never was,
for any database choice.** Bundling was being asked to buy something it could not deliver.

That inverts the cost side of the trade. We are not giving up a silent install to save the database —
there was no silent install to give up. We are giving up *one extra installer wizard* to gain a cluster
that the operating system starts at boot, that runs as an account the shop operator is not, that a real
IT person can service, and that upgrades with a vendor's own tested tooling. The adoption cost is one
more download. The compliance and reliability gain is large.

Once one elevation prompt is accepted for Datum's own service, the marginal cost of PostgreSQL also
being a service is zero. That is also why bundling can return cheaply later, in a different form, if it
is ever worth it (§5).

---

## 1. The version 1 answer

**Confirmed, and sharpened into a permanent boundary.**

> **Datum never owns a database process lifecycle. On any operating system. In any version.**
> The operating system's own service manager owns the daemon — Windows Service, launchd, systemd.
> Datum owns everything above the socket.

Everything above the socket is: probing for a reachable cluster, creating the database, creating the
`datum_app` and `datum_migrate` roles, installing extensions, applying the grants that make the audit
table append-only, running migrations, backup, verification, restore, and the customer-runnable IQ
suite. That is the part where the product's compliance value lives, and it is the part a bundled
cluster was never going to do better.

ADR 0003's escape hatch is promoted to the only supported production path. Its "bundled with the
desktop installer, the user never learns it is there" clause is withdrawn. **The database choice is
untouched** — PostgreSQL only, no dialect abstraction. Deferrable constraint triggers, grant-level
table permissions, recursive CTEs, exclusion constraints on effectivity ranges, and point-in-time
recovery are why the ledger and audit designs work, and every one of them survives intact. SQLite and
PGlite remain rejected on capability, unchanged. The bundling half was the half under attack and the
bundling half is what gives.

**What replaces the installer as the adoption lever.** The thing that loses evaluations is not the
clicking. It is not knowing which PostgreSQL, which version, what password, what a connection string
is, or whether you broke something. Datum's first-run wizard kills all five: it probes for a cluster,
states the exact version range it supports, links the exact download for the operating system it is
running on, waits, and when it sees a cluster it does every remaining step itself and reports that it
verified them. The shop never writes a connection string and never runs `psql`. That is a smaller
promise than "nothing to configure" and it is one we can actually keep.

**The R4 bonus, which is not a consolation prize.** Under bundling, the Windows user who runs the shop
floor owns `PGDATA` and can read the superuser password out of their own `%AppData%`, flip
`pg_hba.conf` to `trust`, and `UPDATE` the audit table. Grant-level append-only would have been a net
for application bugs and nothing more. Under an OS-managed cluster the postmaster runs as a service
account that no operator logs in as, and the GRANT boundary is a real one. The install retreat makes
the compliance claim *stronger*, and we should say so rather than apologise for it.

---

## 2. Success criterion 1, rewritten

Verbatim replacement, to appear in `docs/01-vision-and-scope.md` §7:

> 1. **Install.** On a machine that already runs PostgreSQL 16 or later, a shop installs Datum and is
>    entering data in under ten minutes, on any of three operating systems, with no database
>    administrator and no container runtime. On a bare machine the honest number is thirty minutes,
>    because PostgreSQL is installed first from its own platform installer — one administrator prompt
>    on Windows, one package manager command on Linux, one Homebrew formula on macOS — and the shop is
>    told exactly which version, which download, and what to click. Datum's first-run wizard does every
>    remaining step itself: it finds the cluster, creates the database, the roles and the grants, runs
>    the migrations, and verifies them. Nobody writes a connection string and nobody runs `psql`. An
>    evaluator who wants to see the product before installing anything runs one downloaded file and is
>    looking at seeded data in under five minutes, on a throwaway database that is built so it cannot
>    become a production one.

What this gives up, stated plainly: the old sentence promised something no shipping desktop product
delivers on Windows, and the slice found no counterexample. The new sentence is longer because the
truth is. It still beats the competition it was written against, because a commercial suite arrives
with a consultant and a scheduling call, not with a thirty-minute Saturday.

---

## 3. Does an evaluation path separate from production survive?

**Yes, in two tiers, and yes it is worth the confusion — but only because of how cheaply it comes and
how hard we make it to misuse.**

**Tier 1 — `datum demo`.** A single signed executable per operating system. It unpacks a PostgreSQL via
`postgresql_embedded` (theseus-rs) into a per-user directory on an ephemeral high port bound to
`127.0.0.1`, seeds a demo shop, and opens a browser. No administrator, no service, no firewall prompt
(loopback-only binds do not trigger Windows Defender Firewall's dialog), no container runtime.

This is nearly free because Wave 1 already has to buy it. The kernel crates need a real PostgreSQL for
the ledger property tests before any installer exists, and on the Windows test node without Docker,
`postgresql_embedded` is the only option. Demo mode is that harness plus seed data, a banner, and a set
of refusals. The split costs us a seed fixture, not a packaging product.

**The three refusals that make the split safe.** Metabase's H2 is the cautionary tale — an easy-start
database people ran in production for years. Demo mode is built so that cannot happen:

1. It refuses to import real data. No CSV import, no connection to an external cluster, no restore. You
   can only look at the shipped demo shop.
2. There is no promotion path. `datum restore` and `datum migrate` refuse a demo data directory
   outright. You cannot upgrade a demo into production; you can only throw it away.
3. It self-expires after thirty days and stamps every screen and every rendered document
   **DEMO — NOT A CONTROLLED RECORD**, which also keeps it out of an inspection by construction.

**Tier 2 — `docker compose up`.** One YAML file, `postgres:17` plus `datum-server`. For developers, for
Linux and macOS evaluators who already have Docker, and for our own CI. Labelled evaluation and
development. This is not a production path and is not required of any customer — the vision's
no-container rule is a promise to *customers*, and offering a compose file to people who want one does
not break it.

**Tier 3 — production.** OS-managed PostgreSQL, Datum as a native service. The only path supported for
controlled records. Every document says so in the same sentence every time.

The confusion this causes is real and bounded: three paths, one of which is obviously a toy because it
says so on every screen and refuses to do anything real. The confusion the *alternative* causes —
telling a Saturday evaluator to install a database server before seeing a single screen — costs us the
evaluation outright. The split is worth it.

---

## 4. Each operating system

### Windows — the hard one, and the shop floor reality

Production is PostgreSQL 16 or later installed from the EDB Windows installer, registered by that
installer as the `postgresql-x64-NN` Windows Service under its own service account; and Datum installed
from a signed MSI that requires elevation exactly once, at install, and which registers `datum-server`
as a Windows Service set to **Automatic (Delayed Start)**, creates one inbound firewall rule for the
HTTP port, and writes configuration to `%PROGRAMDATA%\Datum`, not `%LOCALAPPDATA%`. Two elevation
prompts total, both during install, none thereafter.

This is an ordinary Windows server experience. What we are refusing is *silent and no-admin*, which the
slice proved has no prior art. What we get for the refusal: it survives logoff, it starts at boot, the
data directory is machine-scoped so a second office login does not get a second cluster, the path is
short so `MAX_PATH` is not in play, the ACLs are EDB's problem and they solved it, and endpoint
protection sees a binary in `C:\Program Files\PostgreSQL\NN` that it already recognises rather than a
`postgres.exe` writing write-ahead log out of an application's private directory.

Hard rules that fall out, and cost nothing:

- `datum-server` connects to **`127.0.0.1`**, never `localhost`. Windows resolves `localhost`
  IPv6-first, and a `::1` attempt against a v4-only listener costs seconds against a 500ms scan budget.
  This is a constant, not a configurable default.
- PostgreSQL stays loopback-only. Only `datum-server` binds the LAN. One firewall rule, one process.
- Datum never runs `initdb`, so the restricted-token trap, the codepage-versus-UTF8 trap, and the VC++
  redistributable trap all move to a vendor who has already handled them. Datum instead *verifies*
  encoding and collation at first run and refuses to proceed on a non-UTF8 cluster with a specific
  message rather than a mojibake bug six months later.
- Windows is supported for production. We are not repeating Odoo's "discouraged on Windows" hedge,
  because with the operating system owning both services there is nothing left to discourage.

### macOS — two answers, split by role

**Mac mini serving a shop:** `brew install postgresql@17` then **`sudo brew services start
postgresql@17`**. The `sudo` is the whole point — it writes a LaunchDaemon under
`/Library/LaunchDaemons`, which starts at boot and survives logout. Plain `brew services start` creates
a LaunchAgent that dies with the user session, which is the same defect as the bundled Windows cluster
wearing a different hat. Datum installs from a signed `.pkg` whose postinstall lays down
`com.datum.server.plist` as a LaunchDaemon and registers the binary with the application firewall.

**Single-user office desktop Mac:** Postgres.app is acceptable and is the fastest path. It is honest
about what it is — the server stops when you quit the app — and for one person with one Mac that is
fine. It is explicitly not the answer for a machine that serves tablets, and the documentation says so
next to the download link rather than three pages away.

### Linux — the recommended production platform, and we say so

`apt install postgresql-17` or the `dnf` equivalent, then Datum's `.deb` or `.rpm`, whose postinst
creates the system user, installs `datum.service` with `After=postgresql.service`, and installs
`datum.timer` for the nightly backup. systemd is the native lifecycle and reinventing it inside an
installer would be absurd. This is the only platform where the original ten-minute claim is literally
true from a bare machine, and the documentation should recommend it to any thirty-person shop that has
a choice about where the server lives.

---

## 5. Does bundling return?

**In the form ADR 0003 described — Datum's own process tree initializing, starting, stopping and
upgrading a cluster — no. Not in v1, not in v2, not ever.** That is a permanent architectural boundary
and it is written down here so nobody re-opens it every six months. The rationale does not depend on
current tooling maturity, so no future library makes it stale: a system that must survive logoff needs
the operating system's service manager, and once you have the service manager you have no reason to
duplicate it badly.

**A different thing can return, and it is worth naming so it is not confused with the above.** A
*composite installer*: Datum's own installer, already elevated for its own service, invokes the
platform's PostgreSQL installer unattended and then hands the resulting cluster to the operating
system's service manager and walks away. Datum still never owns the lifecycle. This is what turns the
thirty-minute bare-machine number back into ten, and it is a genuinely nice packaging feature.

It may be built when **all five** of these are true, and it is a fresh decision, not an automatic one:

1. Datum ships code-signed installers on all three operating systems from an established signing
   identity. An unsigned composite installer is a SmartScreen wall and an endpoint-protection incident.
2. There is a named packaging owner with a per-OS CI matrix — not a lane borrowed from a feature wave.
   The slice's core point stands: this is a product, not a task.
3. The wizard's manual path has shipped and is documented, so the composite path can fail over to it.
   The composite installer must never be the only way in.
4. There are ten paying production installs, so we know which endpoint-protection products actually
   appear in machine shops rather than guessing from blog posts.
5. It never owns `pg_upgrade`. Major-version upgrades stay with the platform's installer, forever.

If those five are not all true, the answer is no and the discussion is closed.

---

## 6. Backup and restore

The database is the customer's own. Backup ownership splits, and the split is clean:

> **The customer owns the medium, the schedule, and the retention period.
> Datum owns the mechanism, the verification, and the proof.**

`docs/02` §8's claim that backups are a first-class feature with a documented restore drill stays true
under this decision. It is more true, because the drill now runs against a cluster with ordinary
tooling instead of one hidden inside an application.

**What Datum ships in v1** — this closes plan-audit **G9**, which flagged backup as claimed in `docs/02`
and absent from every wave:

- **`datum backup`** — `pg_dump --format=custom` of the Datum database plus a manifest: schema version,
  application version, UTC timestamp, per-table row counts for every audit-relevant table, the ledger
  head, and a SHA-256 of the dump.
- **`datum backup verify <file>`** — restores into a scratch database, re-runs the ledger conservation
  check over every posting group and the audit chain verification, compares the result against the
  manifest, drops the scratch database, and writes a dated verification record into the audit trail.
  This is the step that turns a backup from a file into a record.
- **`datum restore <file>`** — into an empty database; creates roles and grants, restores, then runs
  `verify` and the IQ suite automatically. Never optional, never skippable.
- **Scheduled job registration by the installer** — Task Scheduler on Windows, a LaunchDaemon on macOS,
  a systemd timer on Linux. Nightly backup, weekly verify, to a path the shop chooses in the wizard.
- **A pre-migration automatic backup.** Every application upgrade takes one before running migrations
  and refuses to start if it fails. Bundling never offered this, because there the upgrade engine and
  the database supervisor were the same fragile thing.
- **The written restore drill**, as a document the customer attaches to their own change control.

**What Datum does not own:** where the copy goes, how many generations are kept, how long they are
kept, whether the medium is offsite, and whether anyone ever tested it. Those are the customer's, and
pretending otherwise is how a vendor ends up owning a data loss it cannot fix.

### What a regulated customer needs that an ordinary one does not

This is 21 CFR 11.10(c) — *"protection of records to enable their accurate and ready retrieval
throughout the records retention period"* — and 11.10(e), which requires audit trail documentation to
be retained at least as long as the records it describes.

An **ordinary shop** is done with: nightly `datum backup` to a second physical disk or a NAS, weekly
automatic `verify`, and a restore they tried once. Datum's defaults give them all three.

A **regulated shop** needs five more things, and four of them are theirs rather than ours:

1. **A written retention period derived from their predicate rule**, not from a convenient default. For
   devices under QMSR that is at least the design lifetime of the device and not less than two years
   from commercial release. Datum must therefore never prune anything on its own schedule, and in
   particular must never prune audit rows on a schedule that differs from the records they describe —
   11.10(e) forbids it. *Ours to enforce: Datum has no data expiry feature and will not grow one.*
2. **A bounded recovery point.** A nightly dump means up to twenty-four hours of controlled records can
   be lost, which is a deviation requiring investigation rather than an inconvenience. Regulated
   installations configure PostgreSQL write-ahead log archiving to a second device, bounding the loss
   to minutes. *Theirs to configure on their cluster; ours to document, and to make `verify` work
   against a point-in-time-restored cluster.* This is one of the capabilities PostgreSQL was chosen
   for, and it is available precisely because the cluster is a real one.
3. **Dated evidence of a tested restore.** 11.10(c) is about ready retrieval, not about possessing
   files; an untested backup is evidence of nothing. *Ours: `verify` emits a signed, dated verification
   record into the audit trail, which is the artifact an inspector asks to see. Theirs: scheduling it
   and retaining the records.*
4. **Media control and offsite separation**, under their own SOP. *Theirs entirely.*
5. **A procedural control on superuser access.** This is the honest disclosure. The customer's IT holds
   the PostgreSQL superuser credential, and a superuser can bypass the GRANT-level append-only
   boundary. That boundary is a genuine control against application bugs and against every application
   user — which is more than the bundled design could claim, where the operator *was* the cluster owner
   — but it is not a control against the customer's own DBA. Their validation package needs a
   procedural control there, and our documentation must say so rather than let them discover it at
   inspection. This is the *Part 11-capable product, Part 11-compliant deployment* line the regulatory
   spike drew, applied to backup.

---

## 7. Consequences for Wave 3

Wave 3 was `datum-server`, the Phase 1 modules, the web shell, and a Tauri desktop shell. Bundling was
in scope by implication and out of scope by effort, with zero lanes and zero acceptance criteria —
plan-audit **G26**. That resolves as follows.

**Drops out:**

- **All PostgreSQL lifecycle work.** No sidecar, no `initdb`, no cluster supervision, no port probing,
  no `pg_upgrade`, no antivirus exclusion scripting. It was never staffed and it is now explicitly out,
  in ADR 0003 and in PLAN §10.
- **The Tauri desktop shell.** Its load-bearing justification was carrying the bundled database and
  supervising it. Without that it is an icon and a fullscreen mode, and the architecture's own topology
  already says the floor tablet is a browser. A kiosk-mode browser gives us fullscreen, session lock,
  and scanner input — scanners present as keyboards — at zero packaging cost across three operating
  systems. Tauri returns as its own decision if and only if a shop needs local hardware a browser
  cannot reach: a serial-port gage, a direct label-printer driver, or an offline floor cache. Naming
  that condition keeps Tauri from being re-litigated too.

**Comes in, replacing it, and this is the trade to defend:**

- **`datum-server` as a native service on all three platforms**, with real installers: a signed MSI
  registering a Windows Service and one firewall rule; a signed `.pkg` laying down a LaunchDaemon;
  `.deb` and `.rpm` with a systemd unit ordered after PostgreSQL. **This is the actual reboot-survival
  deliverable**, and it is the one the bundling debate was obscuring.
- **The first-run database setup wizard** in the web shell: probe, per-OS guidance with the exact
  download, create database and roles, install extensions, migrate, apply and verify grants, verify
  encoding and collation, register the backup job. This is the adoption lever now.
- **`datum demo`**, reusing the Wave 1 embedded-PostgreSQL test harness, plus the three refusals.
- **The `docker compose` evaluation file.**
- **`datum backup` / `verify` / `restore` and the scheduled job**, closing G9.
- **`datum iq`**, the customer-runnable installation qualification suite already promised in
  `docs/02` §9.

**Unaffected:** the Phase 1 modules, the web shell, the UI approval gate and its three mockup screens.
The floor terminal screen is still the most important screen in the product; it is now definitively a
browser screen.

**Net effect on Wave 3 size:** it shrinks. Three platform installers and a wizard are less work than a
desktop shell plus a cluster supervisor, and unlike the cluster supervisor they are work with known
prior art on every platform.

---

## 8. The walk-through — (a) through (g)

Times are honest ranges for a thirty-person machine shop's hardware and data volume, on typical shop
broadband. Where a step can prompt, the prompt is named.

| # | Scenario | Steps | Elapsed | Admin prompts |
|---|---|---|---|---|
| a | Saturday evaluation, Windows 11 shop PC, no help | Download `datum-demo-windows-x64.exe` (~90MB) → double-click → unpack, `initdb`, seed → browser opens on `127.0.0.1` | **3–5 min** | **none** |
| b | Thirty-person shop into production, small Windows server | EDB PostgreSQL 17 installer → Datum MSI → first-run wizard → first admin user → backup job | **15–20 min** | **2**, both at install |
| c₁ | Linux box, Ubuntu 24.04 server | `apt install postgresql-17` → `apt install ./datum.deb` → wizard | **6–8 min** | `sudo`, twice |
| c₂ | Mac mini in the office | `brew install postgresql@17` → `sudo brew services start` → `Datum.pkg` → wizard | **10–15 min** (20–25 without Homebrew) | admin password, twice |
| d | Floor tablet reboots overnight, nobody logs in until 6am | Nothing. Both services are Automatic/Delayed Start on the server; the tablet autostarts its kiosk browser | **< 5 s to first scan** | none |
| e | Minor application upgrade | Automatic backup → new installer → migrate on start → service restarts | **2–4 min downtime** | 1 on Windows, `sudo` elsewhere |
| f | PostgreSQL major upgrade, year two, validated install | Customer IT installs PG N+1 alongside → stop Datum → vendor `pg_upgrade` → repoint → `verify` + IQ | **2–4 h window** | yes; it is IT's task |
| g | Machine dies, restore to new hardware, prove intact | Rebuild per (b) → `datum restore` → automatic `verify` + `iq` → signed PDF report | **30–45 min**, proof in the same run | 2, per (b) |

### (a) A machinist-owner evaluates on a Windows 11 shop PC on a Saturday, with no help

They land on the download page and take the big button, which is the demo, not the installer.
Approximately 90MB, a minute or two. Double-click. The executable is code-signed, so SmartScreen does
not wall it; if we ever ship it unsigned they get one "More info → Run anyway" and we deserve it.

It unpacks PostgreSQL into `%LOCALAPPDATA%\Datum\demo`, runs `initdb` with an explicit `-E UTF8`, binds
an ephemeral high port on `127.0.0.1`, seeds a demo shop with parts, routings, lots and a genealogy tree
worth looking at, and opens the browser. `initdb` on a machine with Defender scanning the newly
unpacked binaries is the slow step: **45–90 seconds**, and we show a progress line rather than a spinner
so it does not look hung. Because every bind is loopback, **no firewall dialog fires**. Because nothing
is a service, **no UAC prompt fires**.

**Total: three to five minutes, zero prompts, zero decisions.** Every screen carries the
**DEMO — NOT A CONTROLLED RECORD** stamp and a banner saying the database self-deletes in thirty days.

Honest failure mode: an aggressive endpoint agent quarantines the unpacked `postgres.exe`. The demo
detects the failure and shows a specific message naming the file and linking a one-page document,
instead of a stack trace. That is survivable in an evaluation and would be fatal in production — which
is the clearest illustration of why demo and production are different paths.

### (b) A thirty-person shop puts it into production on a small Windows server

1. Wizard page or documentation link → download the EDB PostgreSQL 17 Windows installer, ~380MB. **3 min.**
2. Run it. **UAC prompt one.** Accept the default directory, set and record a superuser password,
   default port 5432, default locale, uncheck Stack Builder. The installer runs `initdb` and registers
   the `postgresql-x64-17` service. **4–6 min.**
3. Download `Datum-x.y.z-x64.msi`. **1 min.**
4. Run it. **UAC prompt two.** It registers `datum-server` as a Windows Service, Automatic (Delayed
   Start), creates one inbound firewall rule for TCP 8080, and writes configuration under
   `%PROGRAMDATA%\Datum`. **1 min.**
5. Browser to `http://localhost:8080`. The wizard has already found the cluster on 5432. It asks for the
   superuser password once, uses it, and does not store it: it creates the `datum` database, the
   `datum_app` role with no `UPDATE` or `DELETE` on audit, the `datum_migrate` role, installs
   `btree_gist`, runs every migration, applies and then verifies the grants, and confirms UTF8
   encoding. **2 min.**
6. Create the first administrator, set the shop name and time zone. **2 min.**
7. The wizard offers the nightly backup job and asks where it should write. Point it at the NAS. **1 min.**
8. Point tablets and office browsers at `http://server-name:8080`. This works because of step 4's rule.

**Total fifteen to twenty minutes, two administrator prompts, no DBA, and no connection string typed by
hand.** From step 3 onward — the case where PostgreSQL already exists — it is **six to eight minutes**,
which is the ten-minute claim, kept.

### (c) The same on a Linux box, and on a Mac mini in the office

**Linux, Ubuntu 24.04 server.** `sudo apt install postgresql-17` — **2 min**, and systemd has it running
and enabled at boot. `sudo apt install ./datum_x.y.z_amd64.deb` — **1 min**; the postinst creates the
`datum` system user, installs `datum.service` with `After=postgresql.service`, installs the backup
timer, and starts the service. Wizard at `http://host:8080` — **3 min**, and over a local socket it can
use peer authentication so there is not even a superuser password to paste. `sudo ufw allow 8080` if a
firewall is on — **10 seconds**. **Total six to eight minutes from a bare machine**, which is the only
platform where that is true, and the reason Linux is the recommendation for any shop that has a choice.

**Mac mini in the office.** If Homebrew is absent, installing it pulls the Command Line Tools and costs
**5–10 min** before anything else happens; say so on the page rather than letting it ambush someone.
Then `brew install postgresql@17` — **3–5 min** — and `sudo brew services start postgresql@17`, which is
the load-bearing command: it writes a **LaunchDaemon**, so the cluster starts at boot and survives
logout. `brew services start` without `sudo` writes a LaunchAgent that dies at logout, which is the same
defect as a bundled Windows cluster, and the documentation calls that out explicitly. Then `Datum.pkg`,
admin password once, whose postinstall lays down `com.datum.server.plist` as a LaunchDaemon and
registers the binary with the application firewall so the "accept incoming network connections?" dialog
does not ambush an unattended machine. Wizard, **3 min**. **Total ten to fifteen minutes with Homebrew
present, twenty to twenty-five without.**

### (d) A shop floor Windows tablet reboots overnight and nobody logs in until 6am

**Nothing happens, which is the point.**

On the office server, `postgresql-x64-17` and `datum-server` are both Windows Services set to Automatic
(Delayed Start). They come up at boot with no interactive login and stay up through every logoff. The
tablet is a browser client; it autostarts a kiosk browser at `http://server-name:8080` via Windows
Assigned Access or a scheduled task on an auto-login kiosk account — configured once, by whoever set up
the tablet, and documented as an SOP because it is a Windows configuration rather than a Datum feature.

At 6am the operator wakes the tablet and it is on the floor terminal screen. **Under five seconds to the
first scan.** The scan itself stays inside the 500ms budget partly because `datum-server` connects to
`127.0.0.1` and never to `localhost`, so the IPv6-first resolution trap is not in the path.

Under the bundled design this scenario ends with a dead cluster and a machinist in gloves looking at a
connection error, because whoever logged off the office PC on Friday took the database with them.
**This scenario, more than any other, is why the decision goes the way it goes.** The one residual
failure is a shop that powers the office PC off at night — that is an SOP and a note in the deployment
document, and no database architecture fixes it.

### (e) A minor version upgrade of the application

Windows: stop the service, run the new MSI (**one UAC**), which upgrades in place; on start
`datum-server` takes an automatic `datum backup`, refuses to proceed if it fails, runs pending
migrations, and comes up. **Two to four minutes of downtime.** Linux:
`sudo apt install ./datum_new.deb` and systemd restarts it — **about a minute**. macOS: the new `.pkg` —
**two minutes**.

The cluster is not touched. Only Datum's own migrations run, and those are versioned and tested forward
and backward against seeded data per `docs/02` §9. The automatic pre-migration backup is a safety
property the bundled design never offered.

### (f) A major version upgrade of PostgreSQL, two years in, on a validated installation

This is now **the customer's IT task, performed with the platform vendor's tooling, under the customer's
change control** — and that is the correct owner. Datum is not shipping two PostgreSQL major versions
and running unattended `pg_upgrade` as a non-administrator against documentation that says you must be
an administrator, with a rollback nobody has rehearsed. That precise scenario is what ADR 0003's revisit
trigger was written for.

Windows, concretely: run `datum backup` and `datum backup verify`, and keep the report. Install
PostgreSQL 19 with the EDB installer on a second port. Stop `datum-server`. Run EDB's `pg_upgrade` — or
`pg_dumpall` and restore, which is slower, completely adequate at this data volume, and has a far
simpler failure mode. Update Datum's configured port. Start Datum. Run `datum backup verify` against a
fresh backup, and run `datum iq`. Uninstall the old major version once the verification passes, not
before.

**A scheduled two-to-four-hour maintenance window**, of which the compute is minutes. The rest is
planning and verification, which is what it should be. On a validated installation, add a change control
record, the customer's own OQ/PQ re-run, and the signed before-and-after verification reports.

Datum's three obligations, which are the whole of our part: declare a supported PostgreSQL version range
per release; **never require a major bump inside a minor release**; and ship `datum verify` plus the
written drill so that "did everything come across intact" is answered in minutes by a tool rather than
in days by eyeball.

### (g) The machine dies; the shop restores onto new hardware and must prove the records are intact

1. New machine. Install PostgreSQL of the **same major version** recorded in the backup manifest, per
   (b) steps 1–2. **~10 min.**
2. Install Datum at the **same minor version** recorded in the manifest. **~3 min.**
3. `datum restore --from \\nas\datum\2026-09-10.dump`. It creates the database, the roles and the
   grants, then restores. **2–10 min** at single-digit gigabytes.
4. `verify` runs automatically and is not skippable: it re-computes the conservation check over every
   posting group, re-verifies the audit chain, and compares per-table row counts and the ledger head
   against the manifest's SHA-256 and recorded values. It writes a dated verification record into the
   audit trail. **1–3 min.**
5. `datum iq` runs the published installation qualification suite. **2–5 min.**
6. Out comes a signed PDF: backup timestamp, dump SHA-256, schema and application version, row counts by
   table, ledger head, audit chain head, and pass or fail per check.

**Thirty to forty-five minutes to running, and the proof is produced by the same run rather than
assembled afterwards.**

What the auditor is actually asking is 11.10(c), ready retrieval throughout the retention period, and
11.10(e), that the audit trail came across without obscuring anything. The report answers both, and it
answers them with a re-computation rather than an assertion.

What the auditor also asks and **the product cannot answer**: what the retention period is, who
authorised the restore, whether the backup medium was controlled, and whether the restore was performed
under an approved procedure. Those are the customer's SOPs and their validation package. Saying so here
is the same line the regulatory work already drew — Datum is Part 11-*capable*; only a deployment is
Part 11-*compliant*.

And the gap to state before a customer finds it: with nightly dumps the recovery point is up to
twenty-four hours before the failure, so controlled records created that day are gone, and that is a
deviation requiring investigation. A regulated installation configures write-ahead log archiving to a
second device and bounds the loss to minutes. That is §6's item 2, and it is available only because the
cluster is a real PostgreSQL rather than something hidden inside an application — which is the argument
for the database choice restated as an argument for this install decision.

---

## 9. What changes, in three files

1. **`docs/adr/0003-database.md`** — status becomes Accepted, amended. The revisit trigger is recorded
   as fired. PostgreSQL-only is kept. The bundling clause is withdrawn and replaced by the lifecycle
   boundary. The escape hatch is promoted to the production path.
2. **`docs/01-vision-and-scope.md`** — §7 criterion 1 replaced with §2 above.
3. **`docs/02-architecture.md`** — §6 rewritten to state the true install story per operating system,
   the lifecycle boundary, the three tiers, and the backup ownership split.

Consequential changes owned by others, recorded here so they are not lost. **`docs/01` §8 still lists
"whether to bundle a database or require one" as an open question and now contradicts §7 outright; that
question is closed by this decision and the bullet must be deleted.** **`docs/adr/README.md` still lists ADR 0003 as
"PostgreSQL, bundled with the installer / Proposed"; both the title and the status are
now wrong.** Both sit outside this decision's edit permission, so they are handed over rather than
done. PLAN §10 gains "bundled
PostgreSQL lifecycle is out of scope; v1 uses OS-managed PostgreSQL" (G26); Wave 1 `workspace`
acceptance gains a test-PostgreSQL strategy — `DATABASE_URL` passthrough, a GitHub Actions service
container, and `postgresql_embedded` for the Windows node without Docker; Wave 3 swaps the Tauri shell
for the three platform installers, the wizard, demo mode, and the backup commands (G9).
