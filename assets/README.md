# FerrumGate Visual Assets

Visual assets for the [FerrumGate README](../README.md) and supporting docs.
All files are SVG, hand-written, no binary tooling required, and rendered with
the project's existing `system-ui` / `ui-monospace` font stack so they look
correct on any reader (GitHub web, mobile, RSS, static-site export).

## Files

| Asset | Size | Purpose |
|-------|------|---------|
| [`banner.svg`](./banner.svg) | 1280×320 | README hero banner. Wordmark, tagline, three principle chips, and a small gateway schematic. |
| [`lifecycle-flow.svg`](./lifecycle-flow.svg) | 1100×240 | Compact reference of the four-phase execution lifecycle and the minimum lineage chain. |
| [`mark.svg`](./mark.svg) | 256×256 | Square monogram / favicon. Hexagonal iron frame, `FG` monogram, single-use capability key. |

## Visual language

| Token | Light | Dark | Use |
|-------|-------|------|-----|
| Iron blue (primary) | `#4f6b9c` | `#7aa2f7` | Capability flow, system rails, primary strokes |
| Iron oxide rust | `#b8553a` | `#d97757` | Recovery / rollback / terminal, verified output |
| Violet | `#6f5ac9` | `#bb9af7` | Authorization phase, capability tokens |
| Mute (text) | `#1f2330` | `#d8dce6` | Display type, primary line work |
| Muted (text) | `#4a5066` | `#8b92a8` | Tagline, eyebrows, secondary labels |

Each SVG declares its own `prefers-color-scheme: dark` overrides inline, so
both light and dark GitHub renderings read correctly without external CSS.

## Usage

### Banner in the README

```markdown
<p align="center">
  <img src="./assets/banner.svg" alt="FerrumGate — Scoped. Auditable. Reversible." width="100%">
</p>
```

### Lifecycle flow as a section illustration

```markdown
![Execution lifecycle: Compile, Authorize, Execute, Verify and Record](./assets/lifecycle-flow.svg)
```

### Mark as a favicon / social card

```markdown
<img src="./assets/mark.svg" alt="FerrumGate" width="32" height="32" align="absmiddle">
```

For a static-site `<link rel="icon">`, copy `mark.svg` to `static/favicon.svg`.

## Design notes

- **Industrial schematic, not cyberpunk.** All shapes are stroke-driven
  geometry on a transparent background — no glow, no neon, no gradient mesh.
  This matches the project's engineering tone (security engineers and
  platform teams, not consumer).
- **Iron-blue and iron-oxide.** The accent palette is anchored to physical
  metals — cool iron blue (the gate) and warm iron-oxide rust (recovery and
  rollback). Violet is reserved for the authorization phase and the
  capability token.
- **Blueprint grid.** A faint 16-pixel grid sits behind the right side of
  the banner at ~7% opacity. It reads as engineering paper, not as a UI
  pattern.
- **Mid-tone only.** No pure black, no pure white. Strokes sit at the
  `~#2d3344` / `~#c4cad8` range so the assets hold up on either side of a
  theme switch.
- **System fonts.** Display type uses `-apple-system, BlinkMacSystemFont,
  "Segoe UI", system-ui, ...`; mono uses `ui-monospace, SFMono-Regular,
  Menlo, ...`. This avoids the GitHub README's habit of stripping remote
  font links and falling back to a default.
- **Lean files.** Each SVG is hand-written and under ~6 KB so they load
  instantly on a README visit and diff cleanly in PRs.

## When to update

- The wordmark, version tag (`v0.1.0`), or tagline change → edit `banner.svg`.
- The lifecycle adds, removes, or renames a state → edit `lifecycle-flow.svg`
  and the matching `.mmd` diagram in `docs/diagrams/`.
- The brand color or accent system changes → update the `<style>` block in
  every SVG and `site/public/css/main.css`.

See [`docs/diagrams/README.md`](../docs/diagrams/README.md) for the
text-based Mermaid sources of the architecture and lineage diagrams.
