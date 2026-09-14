# Design Guidelines — Wicket

Audience: contributor. Status: partial (interface crate ABSENT).

*Binding on every lane that touches the interface. Where a mockup and this document
disagree, the mockup wins and this document gets amended rather than ignored. The
visual approval gate is signed (section 10).*

---

## 1. Who is looking at the screen

Three genuinely different users, and the most common design failure in this category is
building one interface and giving it to all three.

**The office user** sits at a desktop for hours, works with dense records, and needs
information density above all. Small type is fine. Many columns are fine. They will
learn keyboard shortcuts and resent anything that wastes vertical space.

**The shop floor operator** is standing, possibly wearing gloves, possibly in bright
light or under poor light, and is interrupting physical work to touch the screen. They
have seconds of patience. Density is actively harmful here. This user is why the
product succeeds or fails, because their data entry is what every other screen depends
on, and they will route around bad software without telling anyone.

**The quality and planning user** reads graphs, traces, and exception lists. They need
structure made visible, not more numbers.

Design for the three modes separately. Share the tokens, never the layouts.

## 2. Non-negotiables

- **Dark mode is the default.** Light mode is opt-in only, and only if a user asks.
- **No emojis. Anywhere.** Not in navigation, not in status, not in empty states, not
  in toasts. Status is communicated by shape, color, and words.
- **No lorem ipsum in any mockup or fixture.** Use real manufacturing content. A screen
  designed against realistic part numbers and lot identifiers reveals layout problems
  that placeholder text hides.
- **Consistent button alignment, spacing, and hierarchy.** A destructive action is never
  in the position a confirming action occupies on another screen.
- **Animation only where it is seamless and carries meaning.** State transitions,
  reveals, and graph expansion, yes. Decorative motion, no. Nothing that delays an
  operator.
- **Real compute is multi-threaded and never blocks the interface.** Planning runs,
  genealogy traversal, and report rendering are background jobs with visible progress.

## 3. Color

Dark neutral foundation. Not black, which makes text vibrate, and not navy, which reads
as consumer software.

Semantic roles rather than named colors. A lane implements the token, never a hex value
inline.

| Token | Role |
|---|---|
| `surface.base` | Application background |
| `surface.raised` | Cards, panels, table headers |
| `surface.overlay` | Modals, popovers, menus |
| `border.subtle` | Table rules, dividers |
| `border.strong` | Focus rings, active edges |
| `text.primary` | Values, headings |
| `text.secondary` | Labels, units, metadata |
| `text.disabled` | Inactive |
| `accent` | Primary action, selection, active state |
| `status.ok` | Released, passed, in calibration, available |
| `status.warn` | Due soon, over tolerance, low stock |
| `status.danger` | Rejected, overdue, nonconforming, quarantined |
| `status.info` | Draft, planned, pending |

**Accent is used for action and state only, never for decoration.** A screen where the
accent appears six times has five too many.

**Status colors carry regulatory weight.** Quarantined and rejected material must be
unmistakable at a glance from across a room. Color alone is never the only signal;
every status pill carries a word, because a meaningful minority of machinists are
colorblind and because a photocopied traveler is monochrome.

Contrast floor is WCAG AA for office screens and **AAA for shop floor screens**, which
are viewed at distance, at an angle, and in bad light.

## 4. Typography

One family for the interface, one monospace family for identifiers.

**Part numbers, lot numbers, serial numbers, and quantities are always monospace and
always tabular-figure aligned.** A column of quantities that does not align on the
decimal is a bug. This single rule does more for the perceived quality of an ERP than
any other typographic choice.

Office screens use a compact scale. Floor screens use a scale roughly twice as large
and never use the smallest two steps.

## 5. Layout

Eight-pixel spacing base. Every gap, pad, and inset is a multiple.

**Office screens** use a persistent left navigation rail grouped by module, a record
header carrying identity and status, a tab row for related records, and a content
region. The record header stays fixed when the content scrolls, because knowing which
part you are looking at matters more than one more row.

**Floor screens** have no navigation rail. One task fills the screen. Back is a single
large affordance. There is no nesting deeper than two levels, ever.

**Tables** are the primary element of this product and deserve disproportionate care.
Virtualized rows, server-side pagination and sort, sticky headers, column sizing that
persists per user, inline edit where the domain allows it, and a visible indication of
which columns are filtered. A row is never taller than it needs to be.

## 6. Controls

**Button hierarchy.** One primary action per screen region. Secondary actions are
outlined. Destructive actions are distinct in color and are separated by space from the
confirming action, never adjacent to it.

**Alignment rule.** Primary action sits right in dialogs and forms, left in toolbars.
Pick one and never vary it within the product.

**Floor targets are at minimum 64 by 64 pixels with 16 pixels of separation**, sized for
a gloved finger and an impatient hand. Nothing on a floor screen is smaller.

**Forms** label above the field, never beside. Units are shown adjacent to the value and
are part of the value, never a separate column. Validation is inline and immediate.
Required fields are marked on the label, not by color alone.

**Every state-changing action that requires an electronic signature must say so before
the user commits**, and the signature dialog must state the meaning of the signature in
plain words. This is a regulatory requirement, not a courtesy.

## 7. Status and identity

Status pills carry a word and a shape, not color alone. Shape is consistent across the
product: the same shape always means the same class of state.

An identifier is always rendered the same way everywhere: monospace, same case, same
grouping. A lot number that appears as `LT-2026-0417` on one screen and `LT20260417` on
another is a defect, not a style variation.

## 8. Motion

Transitions are 120 to 200 milliseconds and use a single shared easing curve. Anything
slower feels broken on a shop floor.

Motion earns its place in exactly three situations: revealing structure such as a
genealogy graph expanding, confirming a completed action, and orienting the user during
navigation. Everywhere else, no motion.

**Nothing on a floor screen animates before an operator can act on it.**

## 9. Anti-patterns

Things that will be rejected in audit.

- Hex values or pixel values inline rather than tokens.
- Any emoji.
- A floor screen that borrows an office layout.
- Density achieved by shrinking type below the scale.
- Color as the only carrier of a status meaning.
- A modal inside a modal.
- A spinner with no explanation of what is happening or how long it takes.
- Quantities rendered in a proportional font.
- A destructive action adjacent to a confirming action.
- Placeholder text used as a label.

## 10. Approved visual direction

Owner approved all four mockups as-is on 2026-09-13 19:10:

| File | Screen |
|---|---|
| `design/mockup-shop-floor.png` | shop floor terminal |
| `design/mockup-genealogy.png` | genealogy trace |
| `design/mockup-item-master.png` | item master |
| `design/icon-wicket.png` | application icon |

Accepted as-is. The visual approval gate is signed. An interface crate is ABSENT (no
UI lane starts in this change).

Hex values below are modal (most frequent) samples of named regions in those PNGs.
Anti-aliasing and photographic grain move nearby pixels. A value that cannot be read
from the image is ABSENT, not invented. The three screens do not share one gold; they
are recorded separately rather than averaged.

### 10.1 Item master (`design/mockup-item-master.png`, 1280×720)

| Token | Observed |
|---|---|
| `surface.base` | `#2E2E2E` (content canvas) |
| `surface.raised` | `#1A1A1A` (left nav rail); table header bar `#464B51` |
| `surface.overlay` | ABSENT (no modal in this file) |
| `border.subtle` | table header vs canvas, `#464B51` on `#2E2E2E` |
| `border.strong` | ABSENT |
| `text.primary` | `#FFFFFF` (part number `MDS-450-M4x12`, description) |
| `text.secondary` | nav inactive labels `#727272` |
| `text.disabled` | ABSENT |
| `accent` | `#FAAC3F` (SAVE fill); Released pill and Bill of Material underline sit on the same gold (`#FDAC40` / `#EAAD43`) |
| `status.ok` | Released pill is this gold, not a green |
| `status.warn` | ABSENT |
| `status.danger` | ABSENT |
| `status.info` | ABSENT |
| Typeface family | ABSENT |
| Weight / CSS size | ABSENT |
| Measured cap-height | part number 21 px in this PNG |

### 10.2 Shop floor terminal (`design/mockup-shop-floor.png`, 1280×720)

| Token | Observed |
|---|---|
| `surface.base` | `#252628` |
| `surface.raised` | scan field interior `#28292B`; secondary buttons `#303133` |
| `surface.overlay` | ABSENT |
| `border.subtle` | ABSENT |
| `border.strong` | scan field ring `#FFFFFF`; REPORT QTY / REPORT SCRAP outlines `#FFFFFF` |
| `text.primary` | `#FFFFFF` (`WO-2026-1847`, button labels) |
| `text.secondary` | `#FFFFFF` at lower contrast on the item line; exact muted hex ABSENT (grain) |
| `text.disabled` | ABSENT |
| `accent` | CLOCK ON fill `#BA9352`; CLOCK ON label near-black `#000000` |
| `status.ok` | CERT CURRENT `#95EEAA` (footer; sampled pixels are sparse and grainy) |
| `status.warn` | ABSENT |
| `status.danger` | ABSENT |
| `status.info` | ABSENT |
| Typeface family | ABSENT |
| Weight / CSS size | ABSENT |
| Measured cap-height | `WO-2026-1847` 63 px; `OP 20 TURN` gold 27 px in this PNG |

### 10.3 Genealogy trace (`design/mockup-genealogy.png`, 1280×720)

The title bar in this approved file still reads "Datum". That string is in the mockup;
it is not a second product name.

| Token | Observed |
|---|---|
| `surface.base` | `#1E1F21` (canvas) |
| `surface.raised` | cards `#292E32`; header `#22252A`; right rail `#212429` |
| `surface.overlay` | ABSENT |
| `border.subtle` | card edges on canvas, `#292E32` on `#1E1F21` |
| `border.strong` | muted bronze on query chip, card labels, right-rail edge `#9A8670` |
| `text.primary` | `#FFFFFF` (lot and serial identifiers) |
| `text.secondary` | ABSENT as a clean swatch (small labels, grain) |
| `text.disabled` | ABSENT |
| `accent` | `#9A8670` (bronze, not the item-master gold) |
| `status.ok` | ABSENT as a distinct green; CERTIFIED is bronze on the mill-heat card |
| `status.warn` | ABSENT |
| `status.danger` | ABSENT |
| `status.info` | ABSENT |
| Typeface family | ABSENT |
| Weight / CSS size | ABSENT |
| Measured cap-height | window title 10 px; lot identifiers 14 px in this PNG |

### 10.4 Application icon (`design/icon-wicket.png`, 1013×1013)

Polished gold duck in a rounded square. Not chrome; recorded because it is one of the
four signed files.

| Region | Observed |
|---|---|
| Outer field | `#000000` |
| Frame gold | `#BE863B` |
| Body gold | `#D29942` |
| Highlight gold | `#F0B95E` |
| Interior ground | `#2E2923` |
| Typeface | ABSENT (no lettering) |

## 11. Notes for the next agent

Section 3 is the role names. Section 10 is the values read from the signed mockups.
A shared token file that both the web client and the print and PDF renderer read is
ABSENT. A traveler printed from the system and the screen it was viewed on must agree
once that file exists; do not invent a palette to fill the gap.

Do not start an interface crate from this document. ADR 0009 names the UI stack; the
crate is ABSENT.
