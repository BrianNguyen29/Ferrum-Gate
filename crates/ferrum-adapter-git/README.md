# ferrum-adapter-git

Local git rollback adapter primitives.

Trạng thái:
- prepare captures the current local `HEAD` as `before_ref`
- rollback/compensate reset the repo back to `before_ref`
- verify checks the current `HEAD` against `after_ref` or `before_ref`
- full git mutation execution and gateway wiring remain out of scope for this slice
