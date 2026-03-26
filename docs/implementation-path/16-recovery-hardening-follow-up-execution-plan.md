# 16 — Recovery / Hardening Follow-Up Execution Plan

Planning/status doc for the recovery/hardening follow-up backlog item.
This is planning only; no implementation is claimed as done.

ASCII only.

---

## Backlog Item Source

`docs/implementation-path/08-next-issue-backlog.md` lines 35-37 (P3).

---

## Scope

Two distinct concerns need to be evaluated and scoped before any implementation:

1. **HTTP mutation recovery boundary**
   - If HTTP mutation recovery is extended, clarify the safety boundary before
     any work begins; remote side effects must not be silently claimed as
     rollback-equivalent
   - Grounded in: existing HTTP adapter at `crates/ferrum-gateway/src/adapters/http/`

2. **EmailSend governed-path evaluation**
   - Evaluate whether `EmailSend` should become a first-class supported
     governed path or remain explicitly out-of-scope for v1
   - Grounded in: `crates/ferrum-proto/src/` email types,
     `crates/ferrum-gateway/src/adapters/email.rs`

---

## Out of Scope (This Plan)

- Full HTTP mutation recovery implementation (requires boundary analysis first)
- EmailSend as a governed capability (requires evaluation first)
- Any changes to rollback or compensation logic

---

## Status

**Not started.** This plan exists to document the two evaluation items
so they are not forgotten and so future agents have a clear entry point.

When either item is ready to implement, a dedicated execution-plan doc
(or an amendment to this doc) should be created with concrete commits.

---

## References

| File | Role |
|------|------|
| `docs/implementation-path/08-next-issue-backlog.md:35` | Backlog item source |
| `crates/ferrum-gateway/src/adapters/http/` | HTTP adapter (mutation side) |
| `crates/ferrum-gateway/src/adapters/email.rs` | Email adapter |
| `crates/ferrum-proto/src/` | Protocol types |
