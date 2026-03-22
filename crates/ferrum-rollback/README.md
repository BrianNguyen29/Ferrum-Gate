# ferrum-rollback

Rollback kernel crate.

Vai trò:
- prepare rollback contract
- route qua adapter registry
- verify / compensate / rollback

Trang thai hien tai:
- co rollback service va adapter registry duoc gateway su dung that
- supported adapter set hien tai gom `fs`, `sqlite`, `maildraft`, `git`, `http`, va `noop`
- co adapter-backed recovery evidence cho filesystem, sqlite, maildraft, git, va HTTP verify/no-op rollback boundary
- HTTP remote mutation recovery van conservative no-op; khong claim automated undo cho remote side effects
