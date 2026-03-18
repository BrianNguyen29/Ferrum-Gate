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
- [x] scope mismatch deny test
- [x] single-use capability test
- [x] R3 no auto-commit test
- [x] rollback/compensate test (gateway + fs adapter-backed)
- [x] poisoned context test

## Operator readiness
- [ ] config docs đúng
- [ ] CLI hữu dụng tối thiểu
- [x] lineage usable
- [x] approval flow documented
