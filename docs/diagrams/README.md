# FerrumGate Diagrams

This directory contains **Mermaid source diagrams** (`.mmd`), not binary exports.
Mermaid source is diffable, version-controllable, and can be rendered in many viewers.

## Files

| File | Purpose | Complements |
|------|---------|-------------|
| [`01-architecture-overview.mmd`](./01-architecture-overview.mmd) | High-level component flow and security boundaries | `guides/concepts.md`, `architecture/` |
| [`02-execution-lifecycle.mmd`](./02-execution-lifecycle.mmd) | Intent-to-terminal state machine with rollback outcomes | `guides/concepts.md`, `guides/api.md` |
| [`03-lineage-chain.mmd`](./03-lineage-chain.mmd) | Minimum provenance event chain before a side effect | `guides/concepts.md`, `PRODUCTION_NOTES.md` |
| [`04-deployment-topology.mmd`](./04-deployment-topology.mmd) | Operator-managed deployment layout and boundary choices | `guides/operator.md`, `PRODUCTION_NOTES.md` |

## Rendering

- **GitHub / GitLab**: paste the `.mmd` content into a Markdown file inside a ` ```mermaid ` fence.
- **VS Code**: install the [Mermaid extension](https://marketplace.visualstudio.com/items?itemName=bierner.markdown-mermaid) for live preview.
- **CLI**: use [Mermaid CLI](https://github.com/mermaid-js/mermaid-cli) (`mmdc`):
  ```bash
  npx @mermaid-js/mermaid-cli -i docs/diagrams/01-architecture-overview.mmd -o out.png
  ```

## Maintenance

Update diagram source when any of the following changes:
- Component relationships or security boundaries
- Execution lifecycle states or terminal outcomes
- Provenance event ordering
- Deployment topology or store choices

Do not duplicate long prose from other docs; diagrams should summarize and reference them.
