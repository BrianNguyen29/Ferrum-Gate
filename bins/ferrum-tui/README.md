# ferrum-tui

> **Status**: D.2 TUI MVP — operator convenience only; not production-ready; domainless/waiver scope.
> **Scope**: Lightweight terminal dashboard for existing operator endpoints. No mutation operations.

## Usage

```bash
# Build
cargo build --release --bin ferrum-tui

# Run with defaults (http://127.0.0.1:8080)
./target/release/ferrum-tui

# Run against a specific server
./target/release/ferrum-tui --server-url https://ferrumgate.example.com:8080

# Run with bearer token (token is redacted in UI)
./target/release/ferrum-tui --bearer-token $TOKEN

# Dry-run mode (synthetic OKs, no HTTP calls)
./target/release/ferrum-tui --dry-run

# Custom refresh interval (seconds)
./target/release/ferrum-tui --interval 10
```

## Environment variables

| Variable | Purpose | Fallback |
|----------|---------|----------|
| `FERRUM_TUI_SERVER_URL` | Base URL | `FERRUMCTL_SERVER_URL` → `http://127.0.0.1:8080` |
| `FERRUM_TUI_BEARER_TOKEN` | Bearer token | `FERRUMCTL_BEARER_TOKEN` → unset |

## Keyboard shortcuts

| Key | Action |
|-----|--------|
| `r` | Refresh probes now |
| `?` / `h` | Toggle help overlay |
| `q` | Quit |

## Endpoints monitored

- `GET /v1/healthz`
- `GET /v1/readyz`
- `GET /v1/readyz/deep`

## Non-claims

- **Operator convenience only**: This TUI is a lightweight dashboard for observing endpoint health. It is not a production-ready admin tool.
- **Not a security boundary**: The TUI itself does not enforce auth; it forwards the bearer token to the server.
- **No mutation operations**: MVP is read-only. No approve/reject, token rotation, or policy changes via TUI.
- **Token redaction**: The bearer token is never rendered to the terminal surface or logs.
- **Domainless/waiver scope**: Implemented under the same domainless/waiver scope as the rest of the pilot-tier operator UX.
