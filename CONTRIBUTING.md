# Contributing

## Development flow
1. Read `docs/guides/README.md`
2. Read `docs/guides/concepts.md`
3. Pick one crate or one document boundary at a time
4. Do not change contracts and schemas without updating docs and tests
5. Preserve intent/capability/provenance/rollback invariants

## Commit style
Use conventional commits where possible:
- feat:
- fix:
- refactor:
- docs:
- test:
- chore:

## Pull request checklist
- [ ] Workspace builds
- [ ] Docs updated if behavior changed
- [ ] Contracts updated if enforcement changed
- [ ] JSON Schemas updated if payload shape changed
- [ ] OpenAPI updated if API shape changed
- [ ] New tests added or existing tests updated
