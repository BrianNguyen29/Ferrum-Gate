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
- draft-only trong v1
- delete draft khi compensate

## Git
- before_ref/after_ref rõ
- revert/reset path

## HTTP
- allowlist
- destructive remote mutation coi là R3 nếu chưa có recovery rõ
