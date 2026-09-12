# Spike: Governance, Copyright and Relicensing Risk in Open Source ERP

**Research window:** 11–12 September 2026. Domain redirects, repository states and last-commit dates were verified live inside that window.

**Why this is separate from `spike-landscape.md`.** The landscape document answers "what can this software do." This one answers "who can take it away from you, and how." Those are different questions with different evidence and different consequences. A feature gap costs money to close. A governance failure costs you the codebase.

**The decision this bears on.** If we build, the governance structure is chosen once, before the first line of code, and is effectively irreversible afterwards. Every project in this document that died had the wrong structure in place from the beginning; the ones that survived either got it right at the start or were forked by people who did. There is no evidence in this survey of a project that started with concentrated copyright and successfully dispersed it later.

### Confidence markers

**[SOURCE]** read from a licence, repository or primary document · **[LIVE]** verified by fetching the domain or repo during the research window · **[PRIMARY]** from a first-party interview, announcement or filing · **[UNVERIFIED]** could not confirm, stated as open

---

## 1. The mechanism: the CLA is the relicensing enabler

This is the finding that reorganises everything else in this document. Open-core erosion and acquisition-kills-the-open-edition are not two patterns. They are **one mechanism with two triggers**.

**A software licence can only be changed by whoever holds the copyright.** That is the whole of it. A project's licence tells you what is permitted *today*. Its copyright structure tells you what can be permitted *tomorrow*, and by whom, without asking you.

A Contributor Licence Agreement is the instrument by which a single company accumulates the rights it needs to relicense. Whether a CLA assigns copyright outright or grants a sufficiently broad licence back, the effect is the same: one party ends up holding enough rights to change the terms unilaterally. Contributors sign it because it is presented as a formality about provenance and patent indemnity. It is that. It is also the key to the door.

**Tryton made refusing the CLA an explicit design decision**, and its leads state the reasoning plainly. From a January 2024 interview with Cédric Krier and Nicolas Évrard, published by the Tryton Foundation itself **[PRIMARY]** — https://www.projets-libres.org/n-evrard-amp-c-krier-lerp-tryton-histoire-et-perspectives-par-tryton-fondation/ :

> **Évrard:** *"plus de personnes ont le copyright, plus il est difficile de changer ensuite. Et donc, ça oblige le code à rester open source. C'est aussi pour ça qu'il n'y a pas de CLA."*
> — the more people hold copyright, the harder it is to change later; this forces the code to stay open source, and that is why there is no CLA.

> **Krier, flatly:** *"Si on me demande de signer un CLA, c'est parce qu'on veut changer la licence par après."*
> — if someone asks me to sign a CLA, it is because they want to change the licence afterwards.

They cite the Linux kernel's practical inability to move to GPL-3 as the model. Thousands of copyright holders means no relicensing is administratively possible. **Dispersed copyright is not an accident of large projects; it is a defence that can be chosen deliberately.**

The Tryton Foundation itself exists as a second layer of the same defence. Krier describes the fear among Spanish OpenERP users who were considering the migration: *"que B2CK copie ce qu'avait fait Tiny et prenne le contrôle sur la chose. Et une façon de les rassurer, c'était de créer la fondation"* — that B2CK would copy what Tiny had done and take control of the thing; creating the foundation was a way to reassure them. The foundation holds the trademark and the infrastructure, so the principal commercial vendor cannot capture the identity even though it writes 90% of the code.

**Why this matters to every case in Section 3.** Odoo could relicense AGPL to LGPL at v9 and move Quality, PLM and Shop Floor behind a paywall *because it held the copyright*. Compiere, Openbravo, xTuple and Fedena could close their editions for exactly the same reason. In none of those cases did the community have a legal remedy, because there was nothing to remedy: the copyright holder was exercising rights it genuinely had. The community's only option in every instance was to fork the last freely-licensed snapshot and rebuild the project around it — which is expensive, slow, and succeeds perhaps one time in three.

**Three defensive structures appear in this survey**, and they are not equivalent:

| Structure | Example | How it prevents relicensing | Weakness |
|---|---|---|---|
| **Dispersed copyright, no CLA** | Tryton | Mechanically impossible to relicense without tracing every contributor | Does nothing about *effort* concentration — Tryton is 83% one person's commits |
| **Foundation charter** | Apache OFBiz | ASF's charter forbids proprietary relicensing; contributors retain copyright and grant the ASF a licence via ICLA | Protects the licence, not the project's vitality — OFBiz is feature-frozen with three active committers |
| **Copyleft with network clause** | AGPL projects | Raises the cost of a proprietary fork by a third party | **No protection at all against the copyright holder itself**, which can always dual-licence |

The third is the one most often mistaken for a defence. AGPL restrains *everyone except the party that can change it.* Carbon is AGPL-flavoured and has already carved out `packages/ee`. Axelor is AGPL and gates "updates and maintenance" behind a paid tier. **A strong copyleft licence held by one company is a business model, not a safeguard.**

---

## 2. The copyright ladder

Ranking the projects in this survey by how much unilateral control one party holds, which is the same as ranking them by rug-pull risk:

| Risk | Project | Copyright | CLA | Evidence |
|---|---|---|---|---|
| **Lowest** | **Apache OFBiz** | Contributors, licensed to ASF via ICLA | ICLA required | ASF charter forbids proprietary relicensing. Licence is safe; project vitality is not |
| **Low** | **Tryton** | Deliberately dispersed | **None, by policy** | Stated design decision, quoted in §1 |
| **Low–moderate** | **ERPNext / Frappe** | Frappe Technologies Pvt Ltd, with substantial outside contribution | **[UNVERIFIED]** | GPL-3 app, MIT framework, **no Enterprise edition and no paywalled module** — the revenue model (hosting at $5/mo, not seats) does not require withholding code. Community unease is documented but is about *modularisation into separate apps*, not relicensing |
| **Moderate** | **iDempiere** | Contributors; PMC governance | Apache-style meritocratic PMC | GPLv2-only. Survived the ADempiere stall precisely because governance was plural |
| **Moderate–high** | **Axelor** | Axelor SAS | **[UNVERIFIED]** | AGPL-3, single corporate steward, 409 open issues, and *"updates and maintenance"* already a paid-tier line item |
| **High** | **metasfresh** | metas GmbH | **[UNVERIFIED]** | GPLv2, but **93.3% of recent open issues are vendor staff or bots**, releases stopped publishing at 5.175 (June 2023), and the vendor states in writing it offers no free support. Code is open; the project is not |
| **High — realised** | **Odoo** | Odoo SA | Yes | Already relicensed AGPL→LGPL at v9 and moved the manufacturing-critical modules to a proprietary licence. **This is not a risk; it is a completed event** |
| **Highest** | **Carbon** | Carbon Manufacturing Systems Corp | **[UNVERIFIED]** | Single corporate holder; `packages/ee` commercial carve-out **already in the tree at eighteen months old**; an `UpgradeOverlay` upsell component shipped in the open code; and a licence clause making ordinary internal production use conditional |

---

## 3. Case files

Each case records: who held the copyright, whether a CLA existed, what the event was, **what the community could and could not do**, and current status.

### 3.1 Compiere → ADempiere → iDempiere — the full arc, twice

**Copyright:** Compiere Inc. (founder Jorg Janke), single holder. **CLA:** yes in effect — contributions flowed to the company.
**Licence history:** GPLv2, then a dual-licence arrangement under the Compiere Public License.

**The event, in two stages.** First, in 2006, the community judged that the company had moved away from its open-source posture and forked **ADempiere**. Adaxa, one of the implementers involved, describes it in its own words **[SOURCE]**: *"When the Compiere company was acquired and moved away from its original open-source focus, a group of existing users and implementers (including Adaxa) chose to create a community owned version called ADempiere."* Second, Compiere was acquired by Consona in 2010 and subsequently by Aptean.

**Current status [LIVE]:** `compiere.com` **301-redirects to `aptean.com/en-US/solutions/erp/aptean-industrial-manufacturing-erp`.** The brand no longer exists as a product; the code is inside a proprietary ERP line.

**What the community could do:** fork the GPLv2 snapshot. It did, and ADempiere existed for seventeen years.
**What it could not do:** keep the name, the trademark, the commercial channel, or the original maintainers' attention. And — the part that matters — **forking the code did not fork the governance.** ADempiere reproduced the same structural weakness in a different form: no clear decision-making body, disputed leadership, and a stalling contributor base.

**ADempiere's own end [LIVE]:** `adempiere.net` **does not resolve.** `github.com/adempiere/adempiere` has 883★ and a **last commit of 2023-12-11** — roughly three years stale, not archived, simply over. The manufacturing extension `adempiere/extension_libero_manufacturing` is explicitly marked **[DEPRECATED]**, last pushed 2015.

**Why iDempiere survived where ADempiere stalled.** The 2011 fork (Carlos Ruiz, Heng Sin Low) was **governance-first, not code-first**. It adopted an Apache-style meritocratic PMC with plural committers, a real annual release cadence, and — decisively — an **OSGi plug-in architecture with "2Pack" metadata bundles** that lets third parties extend the system without forking core. Where ADempiere kept the code and lost the decision-making, iDempiere rebuilt the decision-making and inherited the code.
**Status [LIVE]:** last commit 2026-09-10, 661★, GPLv2-only, v13 "Orion" current.

**The lesson, stated precisely:** *the fork that repairs governance survives; the fork that only copies source does not.* This is the single most transferable finding in this document.

### 3.2 Openbravo — the company left the category entirely

**Copyright:** Openbravo S.L.U. **CLA:** yes in effect.
**Event:** progressive narrowing to retail, then absorption. The open-source ERP was not so much closed as abandoned in place while the company became something else.

**Current status [LIVE]:** `openbravo.com` **301-redirects to `orisha.com/en/commerce`.** Openbravo is now a retail and unified-commerce product under Orisha. **The company no longer sells ERP at all.** The official `openbravo/openbravo-erp` repository has **9 stars**, last pushed 2025-02-18.

**What the community could do — and this case is instructive.** The energy that survived did not go into the ERP. It went into the **point-of-sale** component: `kriolos-obiz/kriolos-pos` has 68★ and was active 2026-09-07, and `uniCenta oPOS` (§3.11) is another descendant of Openbravo POS. **The piece with a self-contained, comprehensible user community outlived the enterprise product by more than a decade.** ERP breadth was too large for volunteers to carry; a till application was not.

**What it could not do:** hold the vendor in the category. There is no fork of Openbravo ERP with meaningful activity.
**[UNVERIFIED]:** the exact date the ERP community edition was formally end-of-lifed. Wayback is unreachable from this environment, so the dating rests on repository and domain state rather than an announcement.

### 3.3 xTuple / OpenMFG / PostBooks — never fully open, then deleted

**Copyright:** xTuple (formerly OpenMFG LLC), single holder. **CLA:** yes in effect.

**The sharper finding is that the open edition was never fully there.** OpenMFG shipped under a *"hybrid source code license"* **[SOURCE — Sramana Mitra, 21 April 2009, https://sramanamitra.com/2009/04/21/xtuple/]**, and PostBooks — the nominally open edition — had *"about 85 percent of the functionality of the Standard or OpenMFG editions, which are distributed under standard commercial licenses"* **[SOURCE — Alex Woodie, IT Jungle, 3 March 2009, https://www.itjungle.com/2009/03/03/fhs030309-story01/, quoting Lilly]**.

**The licence was chosen for optics and changed for optics.** The "xTuple License" was an MPL derivative; it was switched to the Common Public Attribution License **two days** after Matt Asay criticised it on CNET on 25 July 2007.

**The closure [LIVE].** `github.com/xtuple/xtuple` and `github.com/xtuple/qt-client` both return **404**. The `xtuple` GitHub organisation retains only leftovers (`oauth2orize-jwt-bearer`, `xtuple.github.io`). Better evidence than the 404s is xTuple's own surviving status page at **https://xtuple.github.io/** : *"xTuple manages our software mostly in private repositories on GitHub. Software can be downloaded by customers and partners from the Commercial Downloads section of xtuple.org."* And `xtuple.org` now 301-redirects to `xtuple.com`, with `files.xtuple.com` and `updates.xtuple.com` no longer resolving. The last publicly mirrored upstream code is from **4 January 2019** (`Pegasus-RPG/xtuple-client`).

**Current status [LIVE]:** `www.xtuple.com` is alive as a closed-source *"modern, cloud-based ERP"* under CAI Software. **The product lived; the open edition was the part that was discarded.**

**What the community could do:** almost nothing. A CPAL-licensed snapshot of a Qt/C++ desktop client, 85% of a proprietary product, with no forkable web successor, attracted no rescue effort. **This is the clearest case in the survey of an open edition functioning purely as a lead-generation channel** — and once an acquirer with an existing sales motion arrived, the channel was redundant.
**[UNVERIFIED]:** IT Jungle gives 2006 for the OpenMFG→xTuple rebrand, which conflicts with contemporaneous eWeek reporting of 30 July 2007; treat 2006 as probably an error. Exact date of PostBooks' formal discontinuation not found.

### 3.4 OpenERP → Odoo — the live, ongoing case

**Copyright:** Odoo SA. **CLA:** yes.
**Event:** at **v9.0 (October 2015)** the Enterprise Edition appeared, the core moved from AGPL to **LGPLv3**, and modules moved behind the proprietary **OEEL-1.0**.

**Odoo SA states the strategy in its own words [SOURCE]** — https://www.odoo.com/blog/odoo-news-5/post/odoo-community-enterprise-532 :
> *"80% of our developments should be open source to attract more users and 20% should be in Odoo Enterprise to improve our revenue stream"*

— with modules selected because a niche is *"easy to monetize."* This is not an accusation; it is the vendor's published plan.

**What moved, and why it matters here.** Verified by probing the public LGPL repo on branch 19.0 **[SOURCE]**: `quality`, `quality_control`, `mrp_plm` (ECO and BoM versioning), `mrp_workorder` (shop floor), `mrp_mps`, `stock_barcode`, `account_accountant`, `documents`, `sign`, `approvals`, `web_studio` are all absent from Community. **Every module a regulated manufacturer needs is on the paid side.**

**And migration is a disclosed revenue line.** Fabien Pinckaers, on the record: *"We used it to monetize Odoo Enterprise Upgrade."* The mechanism requires shipping your entire production database to Odoo, which one customer called *"inacceptable but at that point we had no choice"* — https://news.ycombinator.com/item?id=46439993

**What the community could do:** the **Odoo Community Association** formed and has been genuinely productive — `OCA/management-system` reimplements much of an ISO-9001 QMS, `OCA/manufacture` supplies `mrp_multi_level` and `quality_control_oca`, `OCA/server-tools` supplies `auditlog`, and `OCA OpenUpgrade` provides a free migration path. In July 2026 the OCA launched its own independent apps store.

**What it could not do, and the numbers are stark.** Open "Migration to version X.0" tracking issues across the OCA organisation: **232 for 19.0, 212 for 18.0, 200 for 17.0, 174 for 16.0.** The backlog never clears — v16 modules were still being ported four years on. `OCA/manufacture` carries ~59 modules on 18.0 and **21 on 19.0**; `OCA/management-system` 100 on 18.0 and 82 on 19.0, with the CAPA-effectiveness, complaints and quality-to-manufacturing bridge modules among those that did not make the jump.

**The structural point:** the vendor sets the release cadence, and the community pays the migration tax on every turn of it. A community rescue that is permanently one major version behind is not a rescue for anyone who needs a validated, currently-supported system.

**[NOT ASSERTED]:** a DMCA standoff between Odoo SA and the OCA circulates widely in secondary accounts. **No primary evidence was found. It is not asserted here and should not be repeated.**

### 3.5 SQL-Ledger → LedgerSMB — the consulting-incentive pattern in its purest form

This is the cleanest case in the document, because there was no acquisition, no investor and no relicensing. Just a founder protecting a services income.

**Copyright:** Dieter Simader, sole holder. **CLA:** not applicable — effectively a single-author project.

**The trigger was a security hole.** **CVE-2006-4244**, published 31 August 2006 — session hijack by setting the cookie and the sessionid to the same value. https://nvd.nist.gov/vuln/detail/CVE-2006-4244 . **LedgerSMB's first release came one week later, on 6 September 2006.**

**The cause was the business model.** Jake Edge's account in LWN, 21 March 2007 — https://lwn.net/Articles/227151/ — is the definitive one **[PRIMARY]**:

> *"SQL-Ledger is tightly controlled by its creator, Dieter Simader… the suggested way to get features added more quickly is to pay Simader's company to develop them. In addition, **the documentation, user forums and wiki are only available to those who pay for them**."*

A user had reported the vulnerability nearly a year earlier, across several releases, with no fix. Edge's conclusion states the pattern as precisely as anyone ever has:

> ***"Had Simader been more responsive to those issues, there very well might not be a competing project."***

**What the community could do:** fork, because the licence was genuinely GPL. It did, and it won — but slowly and painfully. LedgerSMB itself nearly died between 2007 and 2011; its own manual records that *"the project looked mostly dead from an outside perspective."*

**Status twenty years on [LIVE]:** SQL-Ledger is stalled at **3.2.12 (January 2023)**, still GPL-2+, and its "What's New" page still advertises a release due **1 February 2025** — nineteen months overdue. LedgerSMB shipped **1.13.8 on 2026-09-11** and commits daily.

**Why this case is load-bearing for us.** It proves the consulting-incentive pattern independently of open-core licensing games. **Paywalling documentation, forums and fixes to protect a services income produced a competitor out of the vendor's own user base**, and the competitor eventually took the project. Any revenue model that makes the maintainer *worse off* when the software is easier to use or self-serve carries this failure mode.

### 3.6 TinyERP → Tryton — the fork that was about access, not code

**Copyright before the fork:** Tiny SPRL. **Licence:** GPL-2.

**The grievance was governance, and it is now verified from primary source [PRIMARY]** — the Tryton Foundation-published interview transcript, https://www.projets-libres.org/n-evrard-amp-c-krier-lerp-tryton-histoire-et-perspectives-par-tryton-fondation/ . Krier:

> *"on a surtout capitalisé sur le fait que nous, on a directement publié le dépôt. Notre dépôt était public, ce qui n'était toujours pas… le cas de TinyERP à l'époque… Il y avait bien un forum, mais les employés de la société n'avaient pas d'autorisation d'aller aider sur ce forum, ça ne passait que par une relation commerciale."*

Two specific complaints: **TinyERP's repository was not public**, and **Tiny's employees were forbidden from helping on the community forum outside a commercial relationship.** Tryton published its repository from day one.

**Timeline:** first commit **19 December 2007**; first release **17 November 2008**, announced on LWN 18 November 2008 — https://lwn.net/Articles/307653/ , which is still live. They forked the public GPL-2 code and upgraded it to GPL-3.
*Discrepancy worth flagging:* the LWN announcement text describes roughly eight months of work; Krier says about eleven. Not material, but do not quote a precise duration.

**The aftermath.** Pinckaers publicly accused them of stealing code; Tryton replied through a lawyer, and — Krier — *"Ça n'a jamais été plus loin."* Later the traffic reversed: Tryton's VAT-number library turned up inside Odoo modules with the copyright attribution stripped. Licence-compliant; attribution not.

**What the fork proved, and what it did not.** It proved that the thing worth forking over was **access, not code** — the repository and the right to help each other. Eighteen years later Tryton is technically excellent and commercially marginal: 83% of last-year commits from one person, **zero service providers in North America**, and a forum archive of 39,628 posts containing zero hits for "MRP", "quality control", "21 CFR", "ISO 13485" or "medical device". Odoo, meanwhile, raised $527M at a $5B valuation. **Forking on governance preserves the freedom. It does not, by itself, build a business or a community.**

**A caution on the defence itself.** Tryton's governance is the best in this survey and it is still under strain: the Foundation President resigned after a year — *"this had been my worst year in the project… I'm phsicologically exhausted"* — and a former board member replied *"All efforts to bring more transparency and turn Tryton to a more community-driven project are blocked"* (https://discuss.tryton.org/t/6950). On tooling friction that a contributor said was deterring contributions, the maintainer response was ***"We are not gonna change it."*** **Dispersed copyright prevents a rug pull. It does not prevent a bottleneck.**

### 3.7 opentaps — dead, and the company left for a different industry

**Founder:** **Si Chen**, Open Source Strategies, Inc.
> **Correction to an earlier internal brief:** opentaps was *not* founded by David Jones. **David E. Jones is an Apache OFBiz co-founder and later the creator of Moqui** — different person, different project. The error did not reach the landscape report, but the record should be clean.

**Copyright:** Open Source Strategies, Inc. **Licence:** AGPL-3.0 layered over Apache-2.0 OFBiz.
**Status [LIVE]:** last release **1.5.0, 23 February 2011**; last SourceForge upload 2013-11-01; *"opentaps 2"* never shipped. `github.com/opentaps/opentaps-1` is **archived**, with substantive work ending 2014-04-09. **`opentaps.org` is now "opentaps – Open Source Climate Solutions"**, a blockchain carbon-accounting project, with the ERP demoted to a single bottom-of-page link — and even the climate blog stopped on 14 June 2023.

**What the community could do:** nothing, and nothing happened. **What is notable is what did *not* flow upstream:** opentaps' CRM was never absorbed into OFBiz — there is no `crmsfa` in `ofbiz-plugins`. An AGPL layer on an Apache base cannot be merged back into the Apache base, so the work was stranded by licence choice even though both projects were open source. **A licence that is more restrictive than its upstream makes your contributions un-donatable.**

### 3.8 Neogia — dead, quietly

**Sponsor:** Néréide (Tours, founded 2004). **Licence:** derived from OFBiz.
**Status [LIVE]:** last release TerCompta 1.4.c, **25 November 2012**. `neogia.org` resolves but serves the **stock Apache2 Debian default page** — the classic signature of a domain kept alive by a forgotten VPS.

**The telling detail:** Néréide is alive and still sells manufacturing ERP services — **on plain Apache OFBiz**. Neogia appears nowhere on their current site, **including their own company history page.** The sponsor did not fail; it simply stopped maintaining its differentiated layer and reverted to upstream.
**[UNVERIFIED]:** no formal end-of-life announcement was found. The conclusion is circumstantial, though consistent.

### 3.9 OpenPro — never open source at all

**Included because it is a distinct failure mode: the trademark of convenience.**
The homepage headline reads *"All-in-One Open Source ERP & Business Software,"* but every substantive claim on the site is about the *stack*: *"Because OpenPro is built using open source technology."* **No licence is named anywhere, and no source is offered to anyone** — not even, as far as can be determined, to paying customers. That is weaker than OpenMFG's hybrid licence, which at least delivered source to customers.
**Trap for anyone re-verifying:** `sourceforge.net/projects/openpro/` is an **unrelated Italian document-management project**, not this company.

**What the community could do:** nothing, because there was never a community — only a marketing adjective. The lesson for us is narrow but real: **"open source" in a product name or headline is not evidence of anything. Read the LICENSE file.**

### 3.10 Fedena — textbook open-core abandonment

**Copyright:** Foradian Technologies (*"Copyright 2011 Foradian Technologies"*). **Licence:** Apache-2.0 for the open edition.
**Status [LIVE]:** `github.com/foradian/fedena` last commit **2012-10-12**; `github.com/projectfedena/fedena` last commits **2016-07-20**. **Neither repository is formally archived, so both look considerably more alive than they are** — a hazard for anyone assessing by eye. `projectfedena.org` now serves the default **"Welcome to nginx!"** page. Meanwhile `fedena.com` thrives as proprietary SaaS claiming 40,000+ schools, with **"open source" absent from its navigation entirely.**

**What the community could do:** fork an Apache-2.0 snapshot, which is maximally permissive. Nobody did at scale. **Permissive licensing is not a community; it is only a permission.** The open edition was a customer-acquisition funnel, and when the funnel was no longer needed it was left to rot rather than formally closed — which is cheaper for the vendor and more confusing for everyone else.

### 3.11 uniCenta oPOS — open-core erosion in progress, right now

**Lineage:** GPLv3, downstream of Openbravo POS. **Copyright:** uniCenta.
**Status [LIVE]:** current release 5.4.0 (August 2025 — none in roughly thirteen months), but the free SourceForge channel is **frozen at 5.0 (2023-12-27)**, and SourceForge itself redirects users to the main site *"for those seeking business support."* On `unicenta.com`, **`/downloads/` renders a login form and `/sources/` returns 404.**

**This is worth watching because it is mid-transition rather than concluded.** The licence is still GPLv3. The *source* and current binaries are behind a membership wall. That combination is legal — GPL obliges you to provide source to those you distribute binaries to, not to the general public — and it is the quietest way to close a project without a relicensing announcement. **A project can become functionally proprietary while its LICENSE file remains unchanged.** Any diligence that stops at the licence file will miss this entirely.

### 3.12 JFire — the vendor simply ceased to exist

**Copyright:** NightLabs GmbH. **Licence:** LGPL-2.0.
**Status [LIVE]:** last release 1.2.0 *"farnsworth"*, 2 December 2011. `jfire.org` and `jfire.net` resolve but refuse connections. `nightlabs.de` still serves a farewell page: *"NightLabs GmbH i.L. … Both are in the state of liquidation since 2015-01-01."*
**What the community could do:** fork an LGPL snapshot. Nobody did. **A single-vendor project does not outlive its vendor by default; it outlives it only if someone was already invested enough to pick it up, and for ERP nobody ever is.**

### 3.13 Adaxa — the brand retired, the company fine

**Status [LIVE]:** `adaxa.com` is live (*"© 2026 Adaxa"*, Melbourne). **The phrase "Adaxa Suite" appears nowhere on it.** The lineup is now iDempiere, ADempiere, and "Ampere" — their trimmed ADempiere for the Australian market. `github.com/adaxa` returns 404, so the Ampere open-source claim **cannot be publicly verified [UNVERIFIED]**.
Included because it shows a gentler outcome: the integrator survived by abandoning its own distribution and standing on a community-governed upstream. **That is the correct move, and it is available only because iDempiere exists.**
**[UNVERIFIED]:** the date the Suite brand was retired.

### 3.14 Apache OFBiz — not a death, a stall, and a different lesson

**Copyright:** contributors, licensed to the ASF via ICLA. **Licence:** Apache-2.0. **Rug-pull risk: effectively zero** — the ASF charter forbids proprietary relicensing, and no single party can change the terms.

**And yet [LIVE]:** current release 24.09.07 (June 2026), **feature-frozen since September 2024**, seven bug-fix-only patch releases since. Roughly 1,023 commits in 52 weeks, but the last 100 commits came from **exactly three distinct human authors**. The manufacturing "Beginner's Guide" on the wiki was last updated **24 March 2009** and still reads *"This is still a WIP."* The user mailing list ran 110 emails across 39 threads in three months, skewing toward proposals to retire components. And the security load is now severe: **19 CVEs in 2026**, including CVE-2026-45434 (CVSS 9.8, pre-auth authentication bypass chainable to RCE) and CVE-2026-50223 (template-directive injection to RCE), with two emergency patch releases in two months.

**The lesson is uncomfortable and important.** OFBiz has the best governance in this survey and the best manufacturing data model — `ProductAssoc` with `fromDate` in the primary key is the only true BOM effectivity dating anywhere in the landscape report, and `EntityAuditLog` captures old and new values natively. **Perfect governance protected the licence and did not protect the project.** Bulletproof copyright structure plus three committers plus nineteen CVEs a year is not a foundation a 30-person device manufacturer can build a validated system on.

**Corollary for us:** governance is *necessary and insufficient*. Choosing the right structure prevents one specific catastrophe. It does nothing about the harder problem, which is sustained effort.

### 3.15 Carbon — the live warning

Treated in full in `spike-landscape.md` §3. Recorded here because it belongs in this sequence.

**Copyright:** Carbon Manufacturing Systems Corp, single holder. **CLA: [UNVERIFIED]** — no CLA document or bot was located, but the structure that matters does not depend on it.
**Licence [SOURCE]** — https://github.com/crbnos/carbon/blob/main/LICENSE :

```
Copyright © 2025, Carbon Manufacturing Systems Corp.
Portions of this software are licensed as follows:
- All content that resides under .../packages/ee and all files that contains a `.ee`
  in this repository require the purchase of a commercial license
- Any use of this software to sell Carbon source code as a hosted service is strictly
  prohibited without obtaining a commercial license.
- Because Carbon is cloud-based software, any use of this software for internal
  production use is strictly prohibited unless the modifications are made open-source
  in accordance with the "AGPLv3" license or a commercial license is obtained.
```

Note also: the README badge and **GitHub's own licence detector both report plain AGPL-3.0, and both are wrong** — the detector matches the bundled AGPL text and misses the carve-out. Anyone doing diligence from GitHub metadata will get this exactly backwards.

**Why it belongs in a governance document rather than only a feature one.** Carbon is eighteen months old and already has: single-company copyright; a `packages/ee` commercial directory in the tree (containing SSO, planning, and every accounting integration); an `UpgradeOverlay` upsell component shipped in the *open* code; and a clause making ordinary internal production use conditional on either publishing your modifications or paying. **The configuration that took Compiere fourteen years and Odoo nine to reach was Carbon's starting position.**

The founder is candid about the motive — *"We open-sourced Carbon not because it's a great business plan, but because that's the system I would have wanted when I was in your shoes"* — and equally candid, in the same memo, about the structural problem underneath the whole category: *"there is no 'perfect' off-the-shelf solution, because each manufacturing business is unique."*

---

## 4. Summary table

| Project | Copyright holder | CLA | Event | Community could | Community could not | Status [LIVE] |
|---|---|---|---|---|---|---|
| **Compiere** | Compiere Inc. | effectively yes | Moved from OSS focus; acquired by Consona 2010 → Aptean | Fork GPLv2 (→ADempiere, 2006) | Keep name, trademark, channel, maintainers | Brand gone; `compiere.com` → Aptean |
| **ADempiere** | contributors | no clear structure | Governance never consolidated | Nothing further | Sustain without a decision-making body | 883★, **last commit 2023-12-11**; `adempiere.net` dead |
| **iDempiere** | contributors, PMC | Apache-style | 2011 fork that fixed governance | Rebuild around plural committers + OSGi | Escape a 35-commit third-party manufacturing plugin | **Alive**, last commit 2026-09-10 |
| **Openbravo** | Openbravo S.L.U. | effectively yes | Pivoted to retail; absorbed by Orisha | Fork the **POS** successfully | Hold the vendor in ERP; no ERP fork survived | `openbravo.com` → orisha.com/en/commerce; repo **9★** |
| **xTuple / PostBooks** | xTuple (ex-OpenMFG) | effectively yes | Hybrid licence from the start; acquired by CAI; repos deleted | Nothing — 85% of a proprietary product, no web successor | Rescue a Qt/C++ client with no community | Repos **404**; product alive and closed |
| **OpenERP → Odoo** | Odoo SA | **yes** | **v9.0, Oct 2015**: AGPL→LGPL, modules to OEEL-1.0 | Form the OCA; reimplement much of it | Ever catch up — **232/212/200/174** open migration trackers | Alive, 54,300★, $5B valuation |
| **SQL-Ledger** | Dieter Simader | n/a (single author) | Docs/forums/fixes paywalled; CVE-2006-4244 unfixed ~1 year | Fork one week after disclosure | Persuade the author to change | **Stalled at 3.2.12 (Jan 2023)**; roadmap 19 months overdue |
| **LedgerSMB** | contributors | — | The fork | Survive a near-death 2007–2011 | — | **1.13.8, 2026-09-11**, daily commits |
| **TinyERP → Tryton** | Tiny SPRL → dispersed | **none, by policy** | Private repo; staff barred from helping on the forum | Fork GPL-2, upgrade to GPL-3, build a foundation | Build a business — 0 NA providers, 83% one committer | Alive, 8.0.9 |
| **opentaps** | Open Source Strategies (Si Chen) | — | AGPL layer over Apache OFBiz; "opentaps 2" never shipped | Nothing; work was **un-donatable upstream** by licence | Merge back into OFBiz | Repo **archived**; domain is now a climate blog |
| **Neogia** | Néréide | — | Sponsor reverted to plain OFBiz | Nothing | — | Last release 2012; domain serves **Apache2 default page** |
| **OpenPro** | OpenPro Inc. | n/a | **Never open source** — "open source" describes only its LAMP stack | Nothing; no source ever offered | — | Trading, proprietary |
| **Fedena** | Foradian | — | Open edition abandoned while proprietary SaaS grew | Fork Apache-2.0 | Nobody did — permission is not a community | Repos stale 2012/2016, **not archived**; SaaS thriving |
| **JFire** | NightLabs GmbH | — | Company liquidated **2015-01-01** | Fork LGPL | Nobody did | Dead; farewell page live |
| **uniCenta oPOS** | uniCenta | — | **Source and current binaries moved behind a login wall**, licence unchanged | Use the frozen 5.0 SourceForge build | Get current source without membership | **In progress**; `/sources/` 404s |
| **Adaxa** | Adaxa | — | Retired its own distribution, stood on community upstream | Move to iDempiere — and did | Verify "Ampere" is open (no public repo) | Company alive; Suite brand gone |
| **Apache OFBiz** | contributors → ASF (ICLA) | ICLA | **No rug pull — a stall** | Nothing to defend against | Sustain: 3 human committers, **19 CVEs in 2026** | Frozen since Sept 2024 |
| **Carbon** | Carbon Mfg Systems Corp | [UNVERIFIED] | **Open-core at birth** | Use the open tree under AGPL reciprocity | Run it internally without publishing modifications or paying | Alive, 2,400★, committed 2026-09-11 |

---

## 5. What communities could and could not do — the capability analysis

Reading across all eighteen cases, the community's options are more constrained than the "you can always fork it" reflex suggests.

**What a community reliably *can* do:**
1. **Fork the last freely-licensed snapshot.** Always available under a genuine OSI licence. It is the floor, and it is real.
2. **Preserve a self-contained component.** Openbravo POS survived twice over (kriolos-pos, uniCenta) while Openbravo ERP did not. Small, comprehensible, single-purpose code with its own user base has an order of magnitude better survival odds than enterprise breadth.
3. **Reimplement withheld functionality**, given a large enough base. The OCA is the proof — `mgmtsystem_nonconformity`, `mrp_multi_level`, `auditlog` all exist because Odoo withheld their equivalents.
4. **Rebuild governance in a fork.** iDempiere is the only unambiguous success in this document, and this is what it did.

**What a community reliably *cannot* do:**
1. **Keep the name, trademark, domain or commercial channel.** Every case. The vendor keeps the identity; the fork starts from zero recognition.
2. **Retain the original maintainers' attention.** In every acquisition case the people went with the company.
3. **Match the vendor's release cadence.** The OCA numbers are decisive: 232 open migration trackers for 19.0, 212 for 18.0, 200 for 17.0, 174 for 16.0. **A rescue that is permanently one major version behind is not a rescue for anyone who needs a currently-supported, validated system.**
4. **Force a maintainer to fix anything.** SQL-Ledger's CVE sat unpatched for a year. The only remedy was exit.
5. **Contribute across an incompatible licence boundary.** opentaps' AGPL work could never go back into Apache-2.0 OFBiz. **A licence more restrictive than your upstream strands your own contributions.**
6. **Undo concentrated copyright after the fact.** No case in this survey shows a project successfully dispersing copyright it had already centralised. The choice is made once.
7. **Generate sustained effort for enterprise-breadth software.** JFire, Fedena and Neogia all had permissive or weak-copyleft snapshots freely available and **nobody forked any of them.** Permission is not a community.

---

## 6. Patterns confirmed and refuted

**CONFIRMED — Open-core erosion.** Odoo stated it as strategy and executed at v9. Axelor gates "updates and maintenance." uniCenta is doing it now without a licence change. **And Carbon shipped the configuration on day one.**

**CONFIRMED — Acquisition kills the open edition.** Compiere → Consona → Aptean; xTuple → CAI; Openbravo → Orisha. In all three the commercial product survived and the open edition was discarded, because the open edition was a lead-generation channel and an acquirer with an existing sales motion does not need one.

**CONFIRMED — Single-vendor bus factor.** Tryton: 83% of last-year commits from one person, 90% from one company. metasfresh: 93.3% of recent open issues from vendor staff, zero outside manufacturing bug reports in two years, and a written statement that the community gets no support. ERPNext users name it directly: *"If the person in charge leaves. Things may start over again."*

**CONFIRMED, and it explains the whole landscape — volunteers will not build unglamorous vertical depth.** Eight mature open-source ERPs, collectively decades old, and **not one has a compliant electronic signature, a calibration system, or UDI support.** iDempiere's entire discrete-manufacturing capability rests on a **35-commit personal repository by two people** while its upstream was deprecated in 2015. The quality modules that exist cluster around whoever had a paying customer: Axelor's is automotive QRQC/8D because Axelor sells to automotive; metasfresh's is catch-weight `weighting/` because metasfresh sells to food; Plex's is APQP/PPAP for the same reason. **Nobody writes a Material Review Board workflow for fun.**

**CONFIRMED — Consulting-revenue incentives work against usability.** SQL-Ledger is the pure case, with a security consequence attached. Axelor's paid "updates and maintenance" tier is a softer instance. A Practical Machinist veteran states the buyer-side perception: *"In general erp is a scam to sell it hours for never ending customisation. Ask any vendor for 2 references who spent less than 100k .. silence."*

**CONFIRMED — No dogfooding flywheel.** The requirements/trace corner of this landscape (doorstop, StrictDoc, Sphinx-Needs, OpenFastTrace) is healthy, actively maintained, and built by developers for developers. The QMS corner, built for quality engineers, tops out at 8 stars. Manufacturing ERP sits with the latter.

**CONFIRMED — Migration cost as a business model.** Pinckaers on the record: *"We used it to monetize Odoo Enterprise Upgrade."* Three years of support per major version and a forced upgrade every two on Odoo Online. ECI deprecating a customer's version one month after billing the renewal is the commercial analogue.

**CONFIRMED — The last 20% differs per shop.** Carbon's founder says it himself: *"there is no 'perfect' off-the-shelf solution, because each manufacturing business is unique."* The low-volume/high-mix shop that abandoned ProShop after $10k: *"Pretty convinced at this stage that we will either have to build our own ERP, or pay somebody else to do it."*

**NEWLY CONFIRMED — The CLA is the relicensing enabler.** §1. This is the mechanism beneath the first two patterns, and it is the one actionable finding.

**REFUTED — "Open source ERP is dying."** ERPNext (39,129★), Odoo (54,300★), Dolibarr (7,598★, V24 announced 11 September 2026), metasfresh, Axelor (five maintained release lines), Tryton, iDempiere and OFBiz **all committed within the last nine days of the research window**. webERP is alive at v5.0.1 with daily commits (canonical repo `timschofield/webERP`, **not** `webERP-team/webERP`, which is stale since 2025-07-05). The category is alive and growing. **It simply does not serve regulated manufacturing.**

**REFUTED — "Acquisition always kills it."** iDempiere is the counterexample, and it proves the mechanism rather than contradicting it: the fork that repaired the *decision-making* survived; the fork that only copied the *source* stalled.

**NOT ASSERTED — "Validation documentation is a services product" as a *cause of death*.** The evidence supports it as a **current market fact** — QT9 and Arena both sell pre-executed IQ/OQ/PQ as headline features, and no open-source project offers any — but **no project in this survey has been documented dying because of it.** State it as a standing structural barrier, not a post-mortem finding. The distinction matters, because it is also why FDA's CSA guidance makes the barrier newly attackable.

---

## 7. What separates survivors from the dead

| | Survivors (ERPNext, Odoo commercially, iDempiere, LedgerSMB, Dolibarr) | The dead |
|---|---|---|
| **Revenue model** | Does not require withholding code — Frappe sells hosting at $5/mo, not seats | Required an open edition as a funnel, which became redundant |
| **Contributor base** | Plural, or a single vendor genuinely still investing | Single vendor whose interest diverged |
| **Extension architecture** | Absorbs the last 20% without forking — Frappe DocTypes, iDempiere OSGi/2Pack, Odoo addons + OCA | Customisation meant forking core |
| **Community's fallback** | Somewhere to go that is not the vendor — OCA, PMC, foundation | Nowhere; the vendor was the project |
| **Governance** | Explicit, written, and survives the founder | Implicit, personal, and did not |

**The sharpest single discriminator is the third one.** Every survivor has a way for a user to add what they need without touching core and without waiting for the vendor. Every death made customisation a fork.

---

## 8. Applying this — the diligence questions

For any open-source ERP we adopt, depend on, or build against, in priority order:

1. **Who holds the copyright, and is there a CLA?** If one company holds it and asks contributors to sign, assume the licence can and eventually will change. Price that in.
2. **Does the revenue model require withholding code?** Hosting, support and warranty do not. Per-seat licensing of the software itself does.
3. **Is there an `ee` directory, an upsell component, or a licence clause conditioning ordinary internal use?** Read the LICENSE file, not the badge, and not GitHub's metadata field — it reported Carbon as plain AGPL-3.0 and it was wrong.
4. **Can I extend it without forking core?** If not, my customisations become an un-maintainable private branch, which is how every stranded implementation in this document started.
5. **Where does the community go if the vendor leaves?** If the answer is "nowhere," the bus factor is one regardless of the star count.
6. **Is current source actually downloadable without an account?** uniCenta shows a project can become functionally proprietary while its LICENSE file stays unchanged.

**And for anything we build ourselves**, §1 and §7 collapse into one decision: **disperse the copyright and refuse the CLA from the first commit, put a neutral body between the vendor entity and the trademark, and make the extension architecture good enough that nobody needs to fork core.** Those three choices are cheap on day one and unavailable on day one thousand. Everything else in `spike-landscape.md` is a feature gap that money and time can close. This is the one that cannot be closed retroactively.

---

## 9. Explicitly unverified, or not asserted

Stated openly rather than omitted, so nobody rediscovers them as gaps later.

**Not asserted — no evidence found:**
- **The Odoo–OCA DMCA standoff.** Circulates widely in secondary accounts. No primary evidence located. Do not repeat it.
- **Validation documentation as a *cause* of any project's death.** See §6.

**Unverified:**
- Openbravo's exact community-edition end-of-life date (Wayback unreachable from this environment).
- The Tryton Foundation's exact December 2012 incorporation date — Wikipedia-only, and its cited source is dead.
- The date Adaxa retired the "Adaxa Suite" name; and whether "Ampere" is genuinely open, since `github.com/adaxa` 404s.
- Any formal Neogia end-of-life notice.
- The verbatim text of the original OpenMFG licence — `openmfg.com` is gone and the archive.li snapshot is CAPTCHA-blocked.
- Whether Carbon, Axelor, ERPNext or metasfresh require a CLA. The structural risk assessment in §2 does not depend on this, but it is the first thing to confirm before depending on any of them.
- IT Jungle's "2006" date for the OpenMFG→xTuple rebrand conflicts with contemporaneous eWeek reporting of 30 July 2007 and should be treated as an error.

**Environment constraints:** `web.archive.org`, `archive.today` (429 + CAPTCHA on every request), G2, TrustRadius and reddit.com were all hard-blocked during this research. Archive mirrors are known to hold the OpenMFG site, the Tryton 1.0 announcement and the Asay CNET post; they could not be retrieved. **Anyone re-running this work from an unblocked network should start there** — those three documents would close most of the remaining gaps above.
