# 13 — Adapter contracts

## Chu trình chuẩn
- prepare
- execute
- verify
- compensate hoặc rollback

## FS
- backup trước mutate
- verify bằng hash
- restore path

## SQLite
- transaction wrapper
- verify predicate / row count
- rollback transaction

## Maildraft
- draft-only (`allow_send=false`) in v1: routes to maildraft adapter for draft create/delete
- `allow_send=true` bindings: explicitly denied at gateway prepare-time (fail-closed); does NOT silently fall through to noop
- `EmailSend` van ngoai scope recovery / unsend trong v1

## Git
- before_ref/after_ref rõ
- revert/reset path

## HTTP
- allowlist
- destructive remote mutation coi là R3 nếu chưa có recovery rõ
