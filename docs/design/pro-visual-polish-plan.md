# Pro Visual Polish — Phased Plan (no code changes yet)

Status: plan only. No files outside this document are modified by this plan.

Companion to `docs/design/viz-improvements-phased-proposal.md` (which covered
dashboard/site content and widget additions). This plan covers visual polish
of the surfaces that already exist: the TUI (`bins/ferrum-tui`), the CLI
(`bins/ferrumctl`), the Zola site CSS, and the public asset story.

Scope verified against the working tree on 2026-09-11:

- `bins/ferrum-tui/src/app.rs` (1785 lines) — 4 tabs, summary-card strip
  (Gauge/Sparkline), 3 tables, 2 overlays (help, approval detail), footer.
  All colors are ratatui 16-color `Color::*` literals scattered through the
  render functions; there is no central theme.
- `bins/ferrum-tui/src/main.rs` — terminal setup/restore at lines 125–157.
- `bins/ferrumctl/src/main.rs` — all output is plain `println!`; no ANSI
  color anywhere. Readiness report at lines 1222–1277; outbox table at
  139–156; DOT lineage renderer at 90–130.
- `configs/monitoring/ferrumgate-grafana-dashboard.json` — 19 panels,
  2-column w=12/h=8 grid, uid `ferrumgate-overview`. **No panel changes are
  proposed in this plan**; the dashboard is listed only so the P1 palette
  decision stays consistent with it.
- `site/static/css/main.css` (238 lines) — CSS variables already defined;
  no `@media` rules; no `:focus-visible` rules.
- `assets/README.md` — canonical palette tokens (iron blue `#7aa2f7` dark,
  iron-oxide rust `#d97757`, violet `#bb9af7`, mute `#d8dce6`, muted
  `#8b92a8`) and the SVG-only, no-binary-tooling stance.

Grounding rule (same as the viz proposal): copy describes only what is
measured and shown. No readiness, Tier-2, SLO, or HA claims anywhere. The
TUI `NON_CLAIMS` block (`app.rs:16–21`) stays verbatim on every surface.

---

## P0 — Polish: smallest viable fixes, no recolor

Each item is independently revertible. Layout intent is preserved: no
widget is moved, resized, added, or removed; changes are text, one restore
call, and two CSS rules.

### P0-1. Row-position indicator on TUI tables
Files: `bins/ferrum-tui/src/app.rs`

- `draw_endpoint_status` (app.rs:771–815), `draw_approvals` (app.rs:817–943),
  `draw_metrics` (app.rs:945–1059).

Today `j`/`k` selection clamps silently at the first/last row and there is
no indication of position within a list. Smallest viable change: extend the
existing block title text with the selected position, e.g.
` Pending Approvals (showing 10, row 3 of 10) ` and
` Metrics Summary (2 of 3 match "http", row 1) `. Title-only; no
`Scrollbar` widget (that would require a layout column and break the
"layout intent preserved" constraint). Approvals paging info already lives
in the title (app.rs:818–842), so this extends an established pattern.

### P0-2. Truncation ellipsis on clipped cells
File: `bins/ferrum-tui/src/app.rs:911–917` (approvals cells truncate with
`chars().take(16)` / `take(20)` silently), footer full-ID line at
app.rs:1196–1203.

Smallest viable change: when a value is cut, render the final character as
`…` so clipped IDs/reasons are visibly clipped. The footer already provides
the full IDs for the selected row, so no information is lost — this only
marks truncation as truncation.

### P0-3. Filter-mode indicator
File: `bins/ferrum-tui/src/app.rs:1165–1220` (`draw_footer`), mode handling
in `bins/ferrum-tui/src/main.rs:427–442`.

While `Mode::Filter` is active there is no visible input line or cursor;
the only cue is the metrics block title (app.rs:956–961), which does not
show that keystrokes are being captured. Smallest viable change: in
`draw_footer`, when `app.mode == Mode::Filter`, replace the hint with
` filter: {metrics_filter}▏  Enter accept · Esc clear `. Reuses the
existing per-tab footer-hint pattern; no new widget.

### P0-4. Clear frame on exit
File: `bins/ferrum-tui/src/main.rs:149–157`.

The restore sequence (`disable_raw_mode` → `LeaveAlternateScreen` →
`show_cursor`) never calls `terminal.clear()`. On terminals that preserve
alternate-screen residue this can leave the last frame visible after quit.
Smallest viable change: one `terminal.clear()?` before
`LeaveAlternateScreen`. No behavior change otherwise.

### P0-5. LOADING/warning yellow review
File: `bins/ferrum-tui/src/app.rs:95` (badge), 853, 975, 1191 (bare
`fg(Color::Yellow)` text).

Current state, verified: the `LOADING` badge is black-on-yellow
(app.rs:95), which is correct contrast and stays. The friction is the three
bare yellow-foreground text spans ("Loading approvals…", "Loading metrics…",
footer transient messages), which are hard to read on light-terminal themes.
Smallest viable change for P0: add `Modifier::BOLD` to those three spans for
scanability and leave the color value untouched. **Any actual recolor of
these spans is deferred to the P1-1 theme decision (D1)** — changing named
colors here would pre-empt that decision.

### P0-6. CLI table dividers
File: `bins/ferrumctl/src/main.rs`

- `print_lifecycle_outbox_list` (main.rs:139–156): header row at 140–143
  has no separator; rows start immediately under it.
- `print_readiness_report` (main.rs:1222–1277): the only rule line is the
  `===` banner at 1224; sections (`health:`, `overall:`, …) are separated
  by blank lines only.

Smallest viable change: one ASCII `-` rule line under the outbox table
header, and one `-` rule under each readiness section label. ASCII (not
Unicode box-drawing) keeps piped/diffed output byte-safe. No color, no
column-width changes.

### P0-7. `pre` block contrast (site)
File: `site/static/css/main.css:150–157`.

`pre` uses `background: var(--color-bg)` (`#0f1115`) — identical to the
page background — so code blocks on default sections are edged only by a
faint `#24263a` border and read as undifferentiated page. Inside
`.section.alt` (`#16181f`) the same border is even fainter. Smallest viable
change: one new token `--color-code-bg` slightly darker than both
backgrounds (e.g. `#0a0c10`) and a one-line swap in the `pre` rule. Text
color and border stay as-is; no palette change.

### P0-8. Keyboard focus ring (site)
File: `site/static/css/main.css` (append).

There are no `:focus-visible` rules anywhere in the 238-line stylesheet;
keyboard users get only browser defaults, which are weak on this dark
palette. Smallest viable change: one rule —

```css
a:focus-visible, .cta:focus-visible {
  outline: 2px solid var(--color-accent);
  outline-offset: 2px;
}
```

Uses the existing `--color-accent`; no new tokens.

---

## P1 — Premium: one theme decision, gated color

### P1-1. TUI Theme struct + optional RGB palette
File: `bins/ferrum-tui/src/app.rs` (rendering only; a `theme` module or a
`Theme` struct near the top of the file).

Colors today are ~30 scattered `Color::*` literals (borders `Blue`, accents
`Cyan`, status `Green`/`Yellow`/`Red`, muted `Gray`/`DarkGray`). Step 1
(no visual change): collect these into a `Theme` struct with semantic slots
(`accent`, `ok`, `warn`, `err`, `muted`, `border`, `badge_fg`) defaulting
to the exact current named colors. Step 2 (opt-in): an RGB palette built
from the canonical tokens in `assets/README.md` (iron blue `#7aa2f7`,
rust `#d97757`, violet `#bb9af7`, mute `#d8dce6`, muted `#8b92a8`) so the
TUI, site, and SVG assets share one palette. Enabled via
`FERRUM_TUI_THEME=rgb` or `COLORTERM=truecolor` detection; the ANSI-16
theme remains the default so non-truecolor terminals are unaffected.

**Decision point D1 (explicit):** this item supersedes the "no recolor"
line in `docs/design/viz-improvements-phased-proposal.md:122` ("No
re-layout, recolor, or re-theming of the existing dashboard or TUI").
Approving P1-1 means amending that line to record that an opt-in RGB theme
was accepted for the TUI only. Without that approval, P1-1 stops at the
Theme struct with ANSI defaults (zero visual change, still worthwhile as
cleanup of the scattered literals — but per the no-adjacent-refactor
guardrail, the struct alone should not land without the palette decision).

**Decision point D2 (minor):** default-off vs. auto-detect for RGB.
Recommendation: auto-detect `COLORTERM=truecolor`, env override wins.

### P1-2. ANSI-gated CLI color
File: `bins/ferrumctl/src/main.rs` (readiness report 1222–1277, audit
verify output, outbox list 139–156).

All CLI output is currently uncolored. Add color with strict gating:
`--color auto|always|never` (clap, default `auto`), `auto` requires
`stdout().is_terminal()` (the crate already uses `IsTerminal`), and
`NO_COLOR` set forces off. Applied semantics are minimal and mirror the
TUI: green/red for VALID/INVALID and approved/rejected states, yellow for
pending/warnings, dim for labels. `json` and `dot` output formats
(`main.rs:45–56`) are never colored. Copy of values is unchanged — only
SGR codes around existing text.

### P1-3. DOT lineage node colors
File: `bins/ferrumctl/src/main.rs:90–130` (`render_dot`).

Nodes are currently unstyled boxes (`node [shape=box]`, line 98). Add a
deterministic `fillcolor` per event kind, mapped from the same
`assets/README.md` hex tokens: violet for policy/capability events, iron
blue for execution/tool-call events, rust for terminal/rollback states.
Output stays deterministic (events already sorted by `event_id`,
line 92–93) and stays valid Graphviz — hex `fillcolor` is a file-format
attribute, not terminal color, so piping is unaffected. Colors map event
kinds only; no status claims.

---

## P2 — Showcase: public-facing polish

### P2-1. TUI demo GIF (VHS)
Files: `assets/` (new `tui-demo.gif` + `tui-demo.tape`), `README.md` (embed).

**Decision point D3 (explicit):** `assets/README.md:5–6` states all assets
are SVG, "no binary tooling required." A VHS-recorded GIF is a binary asset
produced by external tooling; approving P2-1 means recording that exception
for exactly one demo asset, with the `.tape` file committed as the
reproducible source. Recommendation: record against `--dry-run` mode so the
GIF shows the full UI without a live server, and caption it "TUI in dry-run
mode" — grounded, no live-system implication. SVGs remain the primary
assets.

### P2-2. Responsive pass (site)
File: `site/static/css/main.css` (append).

No `@media` rules exist today. The doc grid already uses
`auto-fit minmax(260px, 1fr)` (main.css:176–181), so the gaps are the hero
(`2.25rem` fixed at 90–94), the header flex row (50–61), and wide tables
(196–216) overflowing small screens. Smallest viable change: one media
block at ≤640px — hero `h1` to `1.75rem`, header stacks vertically, and
`table { display: block; overflow-x: auto; }`. One block, three rules.

### P2-3. Public drift guard
Files: new `scripts/validate_public_claims.py`, `Makefile` (`validate`
target).

The repo already runs validators for docs links, monitoring metrics, and
roadmap wording under `make validate`. Add one small script in the same
style asserting that the four `NON_CLAIMS` strings
(`bins/ferrum-tui/src/app.rs:16–21`) appear verbatim in the site
status-banner copy and in the docs that quote them, so public wording
cannot drift from the TUI source of truth. Failure message names the file
and the drifting string. No runtime behavior change.

---

## Decision points summary

| ID | Item | Decision needed | If declined |
|----|------|-----------------|-------------|
| D1 | P1-1 RGB palette | Amend the no-recolor line at `docs/design/viz-improvements-phased-proposal.md:122` for an opt-in TUI RGB theme | P1-1 does not land; P0 yellow spans keep named colors |
| D2 | P1-1 RGB activation | Auto-detect `COLORTERM` (recommended) vs. env-only | Env-only, zero auto behavior |
| D3 | P2-1 VHS GIF | Accept one binary asset + VHS tooling exception to the SVG-only stance in `assets/README.md` | P2-1 skipped; static SVG screenshot instead |

## What this plan deliberately does not do

- No layout changes: tab order, card strip, table columns, Grafana grid
  positions and uid are all untouched (layout intent preserved).
- No changes to Grafana panels, alert rules, or thresholds (covered by the
  viz proposal, and none needed for polish).
- No new dependencies except VHS (P2-1, opt-in tooling) — ANSI color in
  P1-2 uses `crossterm`-style manual SGR codes or the existing dep tree;
  decided at implementation time, gated to the smallest option.
- No changes to `NON_CLAIMS` wording; P2-3 only enforces that other
  surfaces match it.
- No code changes yet. Implementation order within a phase is the listed
  order; each item is one focused commit.
