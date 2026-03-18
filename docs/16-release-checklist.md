# 16 — Release checklist

## Contract integrity
- [ ] contracts cập nhật
- [ ] schemas cập nhật
- [ ] openapi cập nhật
- [ ] docs cập nhật

## Workspace quality
- [ ] cargo check pass
- [ ] fmt pass
- [ ] clippy pass
- [ ] test pass

## Behavior quality
- [ ] scope mismatch deny test
- [ ] single-use capability test
- [ ] R3 no auto-commit test
- [ ] rollback/compensate test
- [x] poisoned context test

## Operator readiness
- [ ] config docs đúng
- [ ] CLI hữu dụng tối thiểu
- [x] lineage usable
- [x] approval flow documented
