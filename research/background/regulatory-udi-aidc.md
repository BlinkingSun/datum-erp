# Regulatory spike — UDI / AIDC addendum

**Date of research:** 2026-09-11
**Companion to:** `spike-regulatory.md` (main report — synthesis, Part 11, QMSR, ISO 13485, DHR/DMR/DHF, CSV)

---

## Provenance caveat

This is delegated fetch work from a dedicated UDI/GUDID/EUDAMED research pass. **I have not independently re-verified each retrieval**, though every claim carries its source URL and the quoted passages are drawn from primary standards and regulatory documents.

Several items originally labelled "corrections" in the research pass were corrections to its own working notes rather than to anything in the main report — the main report never quoted print-quality grades and never claimed GS1 recommends a restricted character subset. Where an item genuinely changes something the main report states, it is flagged as such in §C below.

**Marked throughout:**
- **PRIMARY** — quoted from the standard or regulation named.
- **SECONDARY** — from a vendor, consultancy or derived source.
- **OPEN** — unresolved; do not code against it yet.

---

## A. Net schema changes implied (act on these)

### A1. `package_config.is_sterile_barrier` (boolean), suppressing DI allocation — **changes the main report**

The main report gives 21 CFR 830.50(b) — *"Whenever you create a new device package, you must assign a new device identifier to the new device package"* — as an unqualified rule. GS1's *Healthcare GTIN Allocation Rules* §5.3.5 carves out an exception you will hit immediately. **PRIMARY:**

> "the general rule is that each packaging level requires a separate GTIN. **However, for certain items, particularly sterile items, the multiple barrier packaging is not considered a Packaging Level for GTIN Allocation.**"
> "…the **same GTIN** is used for the item and the 'sterile' barrier package level as the key principles … are the commercialisation of the product … and the function … and **the sterile packaging levels have no impact on commercialisation or function**."

Without the flag, every pouch-in-tray-in-carton device mints two DIs too many.

https://www.gs1.org/docs/gsmp/healthcare/GS1_Healthcare_GTIN_Allocation_Rules.pdf — Release 9.0.2, ratified December 2015. Still current: the **GTIN Management Standard R1.1 (September 2023)** contains **zero** occurrences of "healthcare", "medical" or "pharma", so the two are complementary, not superseding. https://ref.gs1.org/standards/gtin-management/

**Note on table placement:** `is_sterile_barrier` is a property of a *packaging level*, and the packaging hierarchy is a kernel structure (see main report §1.0.2). So this is a module-owned column on a **core** table, not on the item master. Confirm the extension mechanism supports module columns on kernel tables.

### A2. Child-DI derivation is agency-specific — one routine cannot serve all three

- **GS1**: recompute a whole GTIN-14 (indicator digit + 13 digits − old check + new check).
- **HIBCC**: change **one digit** (the Unit of Measure) and recompute the mod-43 check.
- **ICCBBA**: no package-level construct exists in the DI data structure at all.

HIBCC's own guide, **PRIMARY**:

> "the HIBC standard **allows labelers to use the same Product/Catalog Number for all package levels of a device**… The Device Identifier is still unique for each packaging level because **each level is assigned a different Unit of Measure**."

https://www.hibcc.org/wp-content/uploads/HIBCCs-Guide-to-Understanding-Unit-of-Measure.pdf

### A3. `item_master.pcn_compressed`, distinct from the marketing catalogue number

ANSI/HIBC 2.6 §2.1.1, **PRIMARY**:

> "The Product or Catalog Number (PCN) **shall be compressed to eliminate embedded spaces and special characters. Special characters shall not be used in this field. The allowed characters are A through Z and 0 through 9.**"
> "This compression impacts only the machine-readable representations of the PCN and its associated human readable interpretations."

Worked examples from the standard: `655-9`→`6559`, `24-86-2S`→`24862S`, `84/XPG`→`84XPG`, `MP 15 86-G`→`MP1586G`, `92.885*BK`→`92885BK`.

The DI is built from the compressed string; the catalogue number sales uses is a different string. **Caveat for the kernel:** §830.40(c) forbids ever reassigning a DI, and the DI derives from the PCN — so the kernel needs a no-edit, no-reuse invariant on whatever identifier the module derives DIs from.

Field lengths reconciled: **HIBCC UPN / GUDID DI = LIC(4) + PCN(1–18) + U/M(1) = 6–23 characters**; the FDA human-readable field (adds `+`) = 7–24; the full primary symbol (adds the check character) = **8–25** (*"the maximum symbol length is 25 characters"*). FDA's stated "6 to 23" therefore **excludes both the `+` flag and the check character**.

### A4. Check-character handling is inverted between agencies — do not build one shared strip utility

- **HIBCC strips** the check character for EDI and storage. ANSI/HIBC 2.6 §2.1.2 / §2.2.2, **PRIMARY**: *"When using the HIBC data formats in Electronic Data Interchange, **the Check Character is not transmitted or stored in the database**."* And it may legitimately **be a space** (value 38); §4.1: *"The Check Character or Link Character in the symbol will sometimes be a space character. In this case, **the human-readable interpretation shall use an 'underscore' to represent the space character**."* §B.2.1: *"if the link character must be stored or transmitted, the space character should be stored or transmitted explicitly as ASCII decimal 32… Note that some legacy systems and or software are unable to receive and or interpret trailing spaces as part of a data message."*
- **ICCBBA keeps** the data identifiers. ST-001 §6.4, **PRIMARY**: *"**The data identifiers shall be part of the data field** unless the message standard provides an alternative means of unambiguously identifying a data field…"*
- **ICCBBA's DIN check character is never in the barcode at all** — *"is **not** a part of the data content of the DIN data structure and so **is not included in the bar code**"*; it is printed **in a box** for keyboard-entry validation only.

### A5. Case folding must be agency- and structure-aware

- **GS1 CSET 82 permits lowercase.** GenSpecs §3.5.2 on AI (21), **PRIMARY**: *"The serial number field is alphanumeric and **may include all characters** contained in [the CSET 82 table]."* GenSpecs' own healthcare HRI figures deliberately use lower case: `(21) 12345678p901`, `(10) 1234567p`.
- **HIBC is uppercase-only.**
- **ISBT 128 is uppercase-only *except* DS 017** (`=)`, blood-container DI) **and DS 018** (`&)`, container lot), which explicitly permit `{A-Z, a-z, 0-9}` and are left zero-padded.

An ERP that globally uppercases scan input will silently corrupt blood-container catalogue and lot numbers.

### A6. Parser must tolerate a stray `<GS>` that the encoder must not emit

GenSpecs §7.8.6.3, **PRIMARY**:

> "Notwithstanding the above, **the processing routine SHALL tolerate a single separator character immediately following any element string, whether necessary or not**, and process the data in accordance with section 7.8."

i.e. an encoder must not emit `<GS>` after predefined-length AIs such as `(01)`, `(11)`, `(17)`, but a decoder **must accept** one.

GenSpecs §7.8.6.2: *"**Note:** The FNC1 is **not shown** in human readable interpretation."*

### A7. HIBC date parsing must special-case `$$0` and `$$1`

ANSI/HIBC 2.6 Appendix E1.1, **PRIMARY**:

> `0, 1` — **First digit of month in MMYY (month/year) Date format**
> `2` MMDDYY · `3` YYMMDD · `4` YYMMDDHH · `5` YYJJJ · `6` YYJJJHH · `7` Date Field is null, Lot Field follows

For `$$0` and `$$1` **the "indicator" digit IS the first digit of the month** — no separate indicator character is consumed. `+$$09053C001LC` = `$$` + MMYY `0905` (September 2005) + lot `3C001`. Any parser that unconditionally strips one character after `$$` corrupts every MMYY-dated HIBC label. **Hours in `$$4`/`$$6` are G.M.T.**, not local.

### A8. ICCBBA licence status is an operational dependency, not a one-off cost

The FIN Registered Facilities Database and the Product Description Code database (ST-010) live in the **password-protected** area of iccbba.org. **PRIMARY:**

> "If remittance of the annual licensing fee should lapse … access to the password-protected area of the ICCBBA website will be discontinued and the organization will need to re-register."

A lapsed licence means the ERP **cannot resolve FINs or PDCs**. There is no public offline equivalent. Budget it as a hard subscription dependency.

2026 HCT/P medical-device tiers: **US $327.91** (≤1,000 products/yr), **$502.63** (≤5,000/yr), **$502.63 + $0.1615 per product over 5,000**; one-time registration $250 per FIN.
https://iccbba.org/policy-info-and-fees/ · https://iccbba.org/how-to-register/

**By contrast, HIBCC's LIC is a one-time fee.** **PRIMARY:** *"labelers are **not required to pay continuing fees** to acquire and maintain their LIC assignments"*; LICs are *"permanent, never rescinded or reassigned, and globally deployable."* Application page: *"The cost of an LIC assignment is based upon a company's gross annual sales and is a **one-time fee**"* and *"There are **no recurring costs** to maintain your global LIC registration."* Fee schedule (Form C, updated 3/2026): $1,000 (≤$2 M sales) … $20,000 (>$500 M). Non-transferable, non-refundable.
https://www.hibcc.org/udi-labeling-standards/clarification-on-lic-fees/ · https://www.hibcc.org/wp-content/uploads/LIC-Application.pdf · https://www.hibcc.org/lic-application-form-online/

### A9. One minor/major software-change classifier can drive both US and EU registries

GS1 Healthcare Allocation Rules §5.3.3.1, **PRIMARY**:

> "minor changes shall not require a new GTIN. Examples of minor changes include bug fixes, aesthetics, usability enhancements, security patches, or operating efficiency."
> "**A major change … adds to, or changes, functionality and requires a new GTIN.** Examples … new or modified algorithms, database structures, architecture, new user interfaces, or new channels for interoperability."

This is near-verbatim identical to **MDR Annex VI Part C 6.5.2 / 6.5.3**, so one shared classifier can serve both registries.

---

## B. OPEN — resolve before coding

**HIBCC Basic UDI-DI has two conflicting formats in circulation, both hosted on European Commission servers.**

| Source | Format | Check algorithm |
|---|---|---|
| Commission's **current** UDI page, *HIBC Basic UDI-DI* | `++` + LIC(4) + Model Identifier(1–17) + 2-char check — e.g. **`++A999MODELIDENTIFIER11S8`** | **Mod 1021 + Mod 32** (32-char depleted set, `0 1 O I` removed) |
| HIBCC's 2019/2021 EU issuing-entity application annex | IAC **`RH`** + LIC + BDN + 1-char check — e.g. **`RHE999AQ7B5F`** | **Mod-43** |

https://health.ec.europa.eu/document/download/61d21e6a-d0b9-4169-b4ae-8bb8c4cd7d29_en?filename=md_hibcc_basic_udi-di_en.pdf
https://health.ec.europa.eu/system/files/2021-01/application_hibcc_en_0.pdf

The `++` / Mod-1021 document is the one linked from the Commission's *current* UDI topic page, so it is almost certainly the live format and the `RH` version superseded — but **no dated supersession notice was found**, so treat this as open and confirm with HIBCC (udisupport@hibcc.eu) before implementing. The `RH` annex also contains a visible internal error: it says "modulo97" while describing Mod-43, and mis-sums 145 as 144.

Both versions agree the Basic UDI-DI **never appears on packaging**, and HIBCC guarantees no collision by never issuing an LIC beginning `RH`.

---

## C. Corrections and sharpenings to the main report

### C1. The GTIN leading-zero point is sharper than the main report states

The main report says leading zeros are meaningful and to normalize to 14 while keeping the source form. GS1 US UDI Implementation Guideline §2.5.1 adds the directional rule, **PRIMARY**:

> "**Very Important:** A GTIN-12 or a GTIN-13 remains a GTIN-12 or GTIN-13 whether it is in its original 12/13-digit format or represented in a 14-digit format using leading zero(s)… **It is not a GTIN-14.**"
> "**THIS SHOULD NOT BE DONE IN THE OPPOSITE DIRECTION** (i.e., assign a GTIN-14 and remove the first two digits…). A true GTIN-14 … cannot be converted to a 12-digit format because, among other reasons, the check digit … would not match."

And GenSpecs §2.1.1.10: *"**A GTIN-12 may start with one, two or three leading zeroes. These zeroes are meaningful** since they are part of the U.P.C. Company prefix, and therefore these must be preserved when storing the GTIN-12 in a database field."*

→ Store the 14-digit normalized form **plus a `gtin_format` discriminator**. Never round-trip by trimming.

§2.4.3 on anatomy: indicator (1–8; 9 = variable measure) + GS1 Company Prefix + Item Reference *of the contained item* + recalculated check digit, and *"**Although the length of the GS1 Company Prefix and the length of the Item Reference vary, they will always be a combined total of 12 digits in a GTIN-14.**"*

https://documents.gs1us.org/adobe/assets/deliver/urn:aaid:aem:f569d5c4-ab85-4431-ad28-ad350945ab85/Implementation-Guideline-Using-the-GS1-System-for-US-FDA-UDI-Requirements.pdf — R2.3, 3 January 2022

### C2. Character-set restriction is engineering advice, not a spec citation

The main report's normative constraint is correct (§830.20(c) → ISO/IEC 646 invariant set; AI (10)/(21) max 20, CSET 82). If you additionally restrict lot/serial to `[0-9A-Z-]` — which is the right call, see main report §1.0.4 — **do not cite GS1 for it.** No normative GS1 document recommends a restricted subset; GenSpecs says the opposite (see §A5).

The citable hazard is the **GS1 Digital Link percent-encoding list**: 17 of the CSET-82 symbols must be percent-encoded when literal in a Digital Link URI —
`%23 %2F %25 %26 %2B %2C %21 %28 %29 %2A %27 %3A %3B %3C %3D %3E %3F`
https://ref.gs1.org/standards/digital-link/uri-syntax/

### C3. Cite GenSpecs by section, never by figure number

Two releases are live and renumber their figures:

- **R26.0** (January 2026, 579 pp.) uses `Table 7-6` / `Table 7-20`
- **R25.0** (January 2025, 522 pp.) uses `Figure 7.8.5-2` / `Figure 7.11-1`

**Section numbers are identical in both.** Cite sections (§7.8.3, §7.8.4, §7.9.1, §5.12.3.6), never figure numbers.

R26.0: https://www.gs1.org/docs/barcodes/GS1_General_Specifications.pdf (= https://ref.gs1.org/standards/genspecs/, identical 12,175,796-byte file)
R25.0 mirror: https://documents.gs1us.org/adobe/assets/deliver/urn:aaid:aem:afbf55ad-0151-4a0c-8454-d494c0dc9527/GS1-General-Specifications.pdf

### C4. Method note

**govinfo.gov serves CFR text as XML when eCFR blocks automated fetchers**, e.g.
https://www.govinfo.gov/content/pkg/CFR-2023-title21-vol8/xml/CFR-2023-title21-vol8-sec830-50.xml
Combined with the eCFR renderer API and the EU Publications Office cellar service, that covers every primary text without scraping.

---

## D. Symbol quality and verification

**Nothing on this was in the main report.** GenSpecs §5.12.3.6, healthcare label-based minimums, **PRIMARY**:

| Symbol | X-dim min / target / max | Minimum quality |
|---|---|---|
| GS1-128 | 0.170 / 0.495 / 0.495 mm | `1.5/06/660` |
| **GS1 DataMatrix (ECC 200)** | 0.254 / 0.380 / 0.990 mm | **`1.5/08/660`** |
| GS1 DataBar family | 0.170 / 0.200 / 0.660 mm | `1.5/06/660` |
| ITF-14 | 0.170 / 0.330 / 0.660 mm | `1.5/06/660` |

Self-consistency check, GenSpecs §5.6.3.5: *"The aperture is normally specified as being **80% of the minimum X-dimension** allowed for the application."* 80% of DataMatrix's 0.0100″ minimum = 0.008″ → aperture **08**; 80% of GS1-128's 0.0067″ minimum ≈ 0.005–0.006″ → aperture **06**.

### Direct part marking — GenSpecs §5.12.3.7

| Row | X-dim min / target / max | Minimum quality | Applies to |
|---|---|---|---|
| GS1 DataMatrix (label-based) | 0.254 / 0.300 / 0.615 mm | `1.5/06/660` | items **other than** medical devices |
| GS1 QR Code | 0.254 / 0.300 / 0.615 mm | `1.5/06/660` | items other than medical devices |
| GS1 DataMatrix, ink-based DPM | 0.254 / 0.300 / 0.615 mm | `1.5/08/660` | items other than medical devices |
| **GS1 DataMatrix DPM-A** (connected modules — **laser / chemical etch**) | **0.100 / 0.200 / 0.300 mm** | **`DPM1.5/04-12/650/(45Q\|30Q\|30T\|30S\|90)`** | **small medical/surgical instruments** |
| **GS1 DataMatrix DPM-B** (non-connected modules — **dot peen**) | **0.200 / 0.300 / 0.495 mm** | **`DPM1.5/08-20/650/(45Q\|30Q\|30T\|30S\|90)`** | small medical/surgical instruments |

GenSpecs note, **PRIMARY**:

> "GS1 DataMatrix - A is suggested for marking of medical devices such as small medical/surgical instruments. The Minimum X-dimension of 0.100mm is based upon the specific need for permanence in direct marking of small medical instruments which have limited marking area available … with a **target useable area of 2.5mm x 2.5mm and a data content of GTIN (AI 01) plus serial number (AI 21)**."

And: *"**Laser etching is recommended for small instrument marking.**"*

### Grading standards differ by carrier

**PRIMARY:** *"DPM symbols … SHALL be graded to **ISO/IEC TR 29158 (AIM DPM)** and **not ISO/IEC 15415**."*
Linear symbols → **ISO/IEC 15416**. 2D label-based → **ISO/IEC 15415**. Verifier conformance → **ISO/IEC 15426-1 / -2**.

### "1.5" is not "C"

On the ISO/IEC 15415 / 15416 scale, 4.0 = A, 3.0 = B, **2.0 = C**, 1.0 = D. GenSpecs' healthcare tables require **1.5**, i.e. a half-grade *below* C. Quote the full notation, never a letter.

Notation decoded, GenSpecs §5.6.3.5, **PRIMARY**: *"It is shown in the format **grade/aperture/light/angle**"*; aperture is *"the diameter in **thousandths of an inch** … of the synthetic aperture"*; light is *"the peak light wavelength in nanometres"*; angle *"SHALL be included … when the angle of incidence is other than 45 degrees. **Its absence indicates that the angle of incidence is 45 degrees.**"*
So `1.5/08/660` = grade 1.5, 0.008″ aperture, 660 nm red, 45°.

---

## E. GS1 symbology identifiers (decoder-side)

GenSpecs §7.8 and §5.1.3, **PRIMARY**:

> `]C1` = GS1-128 · `]e0` = GS1 DataBar and GS1 Composite · `]d2` = GS1 DataMatrix · `]Q3` = GS1 QR Code · `]J1` = GS1 DotCode

> "The symbology identifier is **not encoded in the barcode but is generated by the decoder after decoding** and is transmitted as a preamble to the data message."

**`]E0` (EAN/UPC) ≠ `]e0` (DataBar/Composite)** — *"Symbology identifiers are **case sensitive**."* Also `]d1` / `]Q1` = plain Data Matrix / QR carrying a GS1 Digital Link URI.

Worked transmission strings: `]d20110012345678902`, `]e00110012345678902`, `]Q30110012345678902`, `]e0011001234567890210ABC123`. **The leading FNC1 never appears in the decoded data** — it surfaces only as the symbology-identifier prefix.

### GS1 Application Identifiers for UDI

From GS1's machine-readable Barcode Syntax Dictionary — https://github.com/gs1/gs1-syntax-dictionary (raw file `gs1-syntax-dictionary.txt`). Column key: `*` = pre-defined length, **no FNC1 separator required**; `N` = numeric; `X` = CSET 82; `..n` = variable length 1..n.

| AI | Meaning | Spec | Length | FNC1 needed? |
|---|---|---|---|---|
| `01` | GTIN | `N14,csum,gcppos2` | fixed 14 numeric, mod-10 check | **No** |
| `10` | BATCH/LOT | `X..20` | **1–20, CSET 82** | **Yes** |
| `11` | PROD DATE | `N6,yymmd0` | fixed 6 | No |
| `17` | USE BY / EXPIRY | `N6,yymmd0` | fixed 6 | No |
| `21` | SERIAL | `X..20` | **1–20, CSET 82** | **Yes** |
| `240` | ADDITIONAL ID | `X..30` | 1–30 | Yes |
| `00` | SSCC (logistics unit) | `N18,csum,gcppos2` | fixed 18 | No |
| `8006` | ITIP (piece of total) | `N14,csum` + `N4,pieceoftotal` | 18 | — |
| `8013` | GMN (= EU Basic UDI-DI) | — | — | — |
| `8014` | HIDRI (= EU Master UDI-DI, made-to-stock) | — | — | — |

The `yymmd0` linter, from GS1's reference implementation `lint_yymmd0.c`, **PRIMARY**: *"ensures that the data represents a meaningful date, in **YYMMDD** format, **additionally permitting YYMM00 format indicating an unspecified day**."*

---

## F. HIBCC — mod-43 check character, fully specified

ANSI/HIBC **2.6 - 2016 (R2026)**, Appendix B.
https://www.hibcc.org/wp-content/uploads/SLS-2.6-Final.pdf
Standalone: https://www.hibcc.org/wp-content/uploads/Mod-43-Check-Character.pdf (FDA's own footnote 7)

**PRIMARY:**

> "The Check Character is the Modulo 43 sum of all the character values in a given message, and is printed as the last character in a given message, preceding the Stop Character. **Leading and trailing asterisk '\*' characters in the human-readable interpretation are not used in calculating the Check Character.**"

Value table: `0-9` = 0–9, `A-Z` = 10–35, `-` = 36, `.` = 37, **space = 38**, `$` = 39, `/` = 40, `+` = 41, `%` = 42.

Canonical worked example (§B.2.0): `+A123BJC5D6E71` → 41+10+1+2+3+11+19+12+5+13+6+14+7+1 = **145**; 145 mod 43 = **16** → `G` → **`+A123BJC5D6E71G`**.

A primary-structure regex is `^\+[A-Z][A-Z0-9]{3}[A-Z0-9]{1,18}[0-9].$` — the trailing `.` must **not** be `\S`, because the check character can be a space.

### Primary Data Structure (= Device Identifier)

`+` flag · **LIC** (4-char alphanumeric, first character always alphabetic, assigned by HIBCC) · **PCN** (1–18 alphanumeric, `A-Z` and `0-9` only — see §A3) · **U/M** (one digit) · check character. Maximum 25 characters.

Unit of Measure digit, ANSI/HIBC 2.6 Table 1: numeric 0–9. `0` always = a single unit; `1`–`8` = ascending packaging levels above unit-of-use; `9` = variable-quantity containers; skipping numbers is allowed; *"U/M identifiers are arbitrarily assigned by each labeler and must be internally consistent."*

Reuse rule, §2.1.3, **PRIMARY**: *"A HIBC Primary Identifier shall not be reissued to any other item, even if the item to which it has been assigned has been discontinued or superseded by another product."*

HIBCC's *Guide to GUDID Device Identifiers* confirms all four GUDID DI roles — Primary, Direct Marking, Unit of Use, Package — are `LIC + PCN + U/M`, differentiated only by the U/M digit (Primary `A999ABC1231`, Unit of Use `A999ABC1230`), and confirms the GUDID field excludes flag and check character: HIBCC Flag `+` / LIC `A999` / Product Code `ABC123` / Unit of Measure `0` / Check Character `V` → *"DI that is entered in the GUDID = **`A999ABC1230`**"*.
https://www.hibcc.org/wp-content/uploads/HIBCCs-Guide-to-GUDID-Device-Identifiers.pdf

### Secondary Data Structure (= Production Identifier)

`+` flag · reference identifier (`$$`, `$$+`, `$`, `$+`, or a 5-digit Julian date) · expiry date field (0 or 4–9 characters, format signalled by the reference digit — see §A7) · lot/batch or serial, 0–18 alphanumeric · **Link Character**.

Appendix E1.3, **PRIMARY**: *"The Link Character for the Secondary Data Structure is the last character from the Primary Data String in the Primary Symbol (Check Character). **The Link Character is not included in concatenated data structures.**"*
Worked pair: primary `*+A123BJC5D6E71G*` / secondary `*+$$52001510X3GD*` — `G` links, `D` checks.

Concatenation, §2.2.1.1, **PRIMARY**: *"a forward slash (/) is used as a delimiter between the primary and secondary data. In addition, **the primary data Link Character, the plus (+) at the start of the secondary data, and the secondary data Link Character are omitted. Only one Check Character at the end of the symbol will be used** which will check the entire data string."*
Example: `+A99912345/$$52001510X3 3`

Asterisks in linear HRI (`*…*`) are Code 39 start/stop — not encoded, not counted in the mod-43 sum.

### HIBCC does not use FNC1 or Application Identifiers

Full-text search of ANSI/HIBC 2.6 finds no occurrence of `FNC`, "function 1", "application identifier" or "GS1". HIBC self-identifies **structurally**, via §2.2.1 Note 1, **PRIMARY**:

> "The HIBC Secondary Data Structure is distinguished from the Primary Data Structure in that **the Primary Data Structure has an alphabetic character following the HIBC Supplier Labeling Flag Character '+', while the Secondary Data Structure has a numeric character or a '$'** following the flag character."

Consequence for scanner configuration: GS1 AIM-prefix and `<GS>`-translation settings do nothing for HIBC. Parse on `+` / `$` / `/`.

### HIBCC symbologies — five, not three

ANSI/HIBC 2.6 §3.1–3.2: **Code 128** (ISO/IEC 15417), **Code 39** (ISO/IEC 16388, *Regular setting, not Full ASCII*, reader's Full-ASCII function disabled, 3:1 wide:narrow), **Aztec Code** (ISO/IEC 24778), **Data Matrix ECC200** (ISO/IEC 16022), **QR Code** (ISO/IEC 18004).
Print quality: linear ≥ `C/06/660` per ISO/IEC 15416 (target `B/06/660`), X-dim 0.010″ target / 0.0067″ minimum, quiet zones ≥10X; 2-D ≥ `C/06/660` per ISO/IEC 15415, X-dim 0.015″.

### FDA deprecated two HIBCC delimiters in v1.4

Change log of https://www.fda.gov/media/96648/download (v1.4, 1 September 2025), **PRIMARY**:

> "HIBCC Updates: Updated `$` Lot Number data delimiter to indicate **'for backward compatibility only'**. Updated `$+` Serial Number: data delimiter `$+` updated to indicate 'for backward compatibility only'."

→ **Emit `$$7` and `$$+7`. Still parse `$` / `$+` on input.**

---

## G. ICCBBA / ISBT 128

Not covered in the main report at all. Verified against **ST-011 v1.9.0 (April 2024)** and **ST-001 v6.2.2**.

Scope boundary, ST-011 §7.6, **PRIMARY**:

> "the scope of ICCBBA is limited to coding MPHO. In the United States, this means that **only those medical devices with an HCT/P component will be addressed in ISBT 128 coding. Labelers that distribute devices without an HCT/P component should contact one of the other issuing agencies.**"

### Device Identifier = Processor Product Identification Code, DS 034, data identifier `=/`

16 data characters: **FIN** (5 chars from `{A-N, P-Z, 0-9}` — note **no letter `O`**) + **FPC** (6 chars `{A-Z, 0-9}`, leading-zero padded, default `000000`) + **PDC** (5 chars `{A-Z, 0-9}`, from the ICCBBA database).

**Uniqueness rule — bolded in the standard, a hard ERP constraint, PRIMARY:**

> "For all HCT/P, the combination of the ISBT 128 **Donation Identification Number, PDC, and Product Divisions Code** shall create global uniqueness. **The FPC shall not be used as a complete or partial alternative to any of these data elements because it is not standardized.**"

### Donation Identification Number, DS 001, DI `=` + next character

This is the §1271.290(c) distinct identification code. **PRIMARY:**

> "**Note: This is the only data structure in which the second character of the data identifier shall be part of the data content.**"

15 data characters (`✦ppppyynnnnnnff`), of which **only the first 13 are the DIN**; *"The last two characters are flag characters and should be ignored."* For devices, *"**the value of ff shall be set to 00**."* The DIN *"shall be **globally unique for a one hundred year period**."*

Its check character uses **ISO/IEC 7064 Mod 37-2** and *"is **not** a part of the data content of the DIN data structure and so **is not included in the bar code**"*; it is printed **in a box** for keyboard-entry validation only.
IG-033: https://iccbba.org/wp-content/uploads/2025/08/83d6e1_55ed0f143bdb4c07aa7ad47135e3a8e9.pdf

### Other structures

- **Serial = Product Divisions**, DS 032, DI `=,` — 6 chars `{A-Z, 0-9}`; ***`000000` is forbidden***; numeric `000001`–`999999` recommended.
- **Lot = MPHO Lot Number**, DS 035, DI `&,1` (a **three**-character data identifier) — ≤18 chars; *"**Only upper case alphas may be used in this data structure when it is used within a PI.**"*
- **Expiry `=>` / Manufacturing `=}`** — `cyyjjj` (century + 2-digit year + Julian day) = FDA's `YYYJJJ`.

### Compound Message, DS 023, DI `=+`

How the whole UDI rides in one symbol: `=+aabbb`, where `aa` = count of following structures and `bbb` = `000` (unspecified) or an RT017 sequence id. FDA-compliance rule, **PRIMARY**: *"the compound message shall always begin with the Processor Product Identification Code [DS 034] (the DI) … the Product Divisions [DS 032] and Donation Identification Number [DS 001] shall also be present… **if non-UDI data structures are included in the compound message, they must appear after the PIs.**"*

**ICCBBA now encourages unspecified messages**, but readers must handle both. Eye-readable rule §6.2.2: the compound-message header `=+04035` is **omitted** from the printed plain text.

Example (79 chars HR / 67 DB):
`=/A9999XYZ100T0476=,000025=A99972312345600=>025032=}023032&,1000000000000XYZ123`
Same content as one DataMatrix compound message:
`=+06000=/A9999XYZ100T0476=,000025=A99972312345600=>025032=}023032&,1000000000000XYZ123`

### Symbologies and EDI

**Code 128 only** (linear, ISO/IEC 15417, quality `1.5/6/670`, target X-dim 0.25 mm) and **Data Matrix ECC200** (ISO/IEC 16022). No FNC1, no GS1 AIs.

EDI convention is **the opposite of HIBCC's** — see §A4.

Sources: https://www.isbt128.org/ST-011 (302 → https://iccbba.org/wp-content/uploads/2025/08/1a7593_4fb5fc74848245a0b44cbb325172261e.pdf) · ST-001 v6.2.2 https://iccbba.org/wp-content/uploads/2025/08/1a7593_1d979d39efc64822a9880496c2ebe4ef.pdf · ST-023 base labels https://www.isbt128.org/ST-023 · library https://iccbba.org/technical_library/

*Note: iccbba.org and isbt128.org return HTTP 403 without a browser User-Agent. ICCBBA documents are licensed — the ERP team should work from a licensed copy.*

---

## H. GTIN allocation triggers — the §830.50(a) analogue

**GS1 Healthcare GTIN Allocation Rules, Release 9.0.2, ratified December 2015.** PRIMARY.

§5.3.1 — the direct §830.50(a) analogue:

> "**Any change to the product form, fit, or function, in addition to differences or changes in intended use requires a new GTIN.**"
> "compliance to regulatory requirements always takes precedence"

§4.4 — the §830.50(b) analogue:

> "different levels within a hierarchy … are assigned different GTINs … **each different grouping of the same item requires a separate GTIN**."

§5.3.5 — the sterile-barrier exception: see §A1.

§5.3.4 certification marks: adding a mark not previously shown *"requires a new GTIN for markets where the certification mark is of particular relevance"* — but adding one purely to enter a new market has no effect where the product already sold.

§4.1.1: primary and secondary packaging in a 1:1 relationship *"may have different GTINs assigned when required by regulation or as agreed within a trading partner relationship."*

§5.2.1 (combination / drug-device): *"Any change to the Regulatory Filing of a product … will lead to new GTIN."*

**GTIN Management Standard R1.1** adds ten general triggers; device-relevant ones: §2.1 new product; §2.2 declared formulation/functionality (**both** conditions must be met); §2.3 declared net content; **§2.4 a change of over 20% to a physical dimension on any axis, or gross weight** — with the anti-gaming clause *"**Frequent cumulative changes, without changing the GTIN, in avoidance of the 20% rule is an unacceptable practice**"*; §2.5 certification mark; §2.6 primary brand; **§2.8 pack/case quantity**; §2.9 predefined assortment.

Guiding principles: *"**At least one of the guiding principles must apply for a GTIN change to be required**"*, and *"**All local legal and regulatory requirements supersede the GTIN Management Standard.**"*

---

## I. Two more GS1 facts that affect label design

**GS1 QR Code is not used for UDI.** GS1 US UDI Implementation Guideline §4.1, **PRIMARY**:

> "A GS1 DataMatrix is often mistaken for a GS1 QR Code. While they may visually look similar, they are two separate and distinct barcodes… **The GS1 QR Code is not used in regulated healthcare for UDI.**"

**Dual-barcode rule for retail-sold devices**, §4.2.2, **PRIMARY**: Class II/III devices sold at retail need EAN/UPC **plus** a GS1-128 / DataMatrix / DataBar carrying *"both the same GTIN … and the production information"* — *"While this practice may be redundant, this very redundancy assures users that the information is correct"* — and *"the second barcode **must be on the same side of the packaging** as the primary EAN/UPC barcode."* Otherwise: *"As a best practice, apply only **one** barcode at each packaging level."*

**AI (20) VARIANT** *"SHALL NOT be used where the variation would trigger the allocation of a different GTIN per the GTIN Management Standard."*
**AI (240) ADDITIONAL ID** is the sanctioned place for legacy catalogue numbers: *"a cross-reference to previously used catalogue numbers… However, it must not be used to replace the GTIN."*

**GS1 Master UDI-DI (EU optics):** for made-to-stock devices already GTIN-identified, **HIDRI AI (8014)**; for made-to-order devices not currently GTIN-identified, **Made-to-order GTIN AI (03)**. https://www.gs1.org/industries/healthcare/udi

GS1 is an accredited or designated issuing agency in the **EU, Brazil, China, Egypt, Saudi Arabia, Singapore, South Korea, Taiwan, Türkiye, USA and Australia** — relevant if the ERP has a multi-market roadmap.

---

## J. Open gaps

1. **OPEN — HIBCC Basic UDI-DI format** (§B). Confirm with HIBCC (udisupport@hibcc.eu) before implementing.
2. **NOT RETRIEVED — the GUDID Data Elements Reference Table itself.** 21 CFR 830.310 (quoted in full in the main report) remains the authoritative element list until you obtain it from FDA directly.
3. **NOT RETRIEVED — the text of Regulation (EU) 2024/1860.** EUR-Lex blocks automated fetch. The EUDAMED module dates below therefore rest on the Commission's overview page, not on the regulation.
4. **SECONDARY — EUDAMED status.** Per https://health.ec.europa.eu/medical-devices-eudamed/overview_en : **four modules became mandatory 28 May 2026** — Actor registration (voluntary since December 2020), UDI/Device registration (voluntary since October 2021), Notified Bodies & Certificates (voluntary since October 2021), and Market Surveillance. The trigger was **Commission Decision (EU) 2025/2371**, published 27 November 2025, starting the six-month transition provided by Regulation (EU) 2024/1860. Clinical Investigations & Performance Studies is under analysis; Vigilance & Post-Market Surveillance is in development, and the Commission notes *"There will be no time for voluntary use for these two modules before they become mandatory."*
5. **PARAPHRASE — ISO 13485 clause text** throughout the main report. Buy the standard before printing clause language.
6. **SECONDARY — GAMP 5 Second Edition** appendix designations and software-category count in the main report.
7. **NOT INDEPENDENTLY RE-VERIFIED** — every fetch in this addendum was performed by a delegated research pass; URLs are recorded but the retrievals were not repeated.
