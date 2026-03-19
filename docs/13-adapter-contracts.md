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
- `prepare` capture local `HEAD` thanh `before_ref`
- `execute` hien chi chap nhan `payload.after_ref` va fail-closed neu khong khop `HEAD`
- `verify` doi chieu `after_ref` da duoc persist tu execute-time metadata, hoac fallback `before_ref`
- `rollback` / `compensate` reset repo ve `before_ref`
- gateway `prepare` da route mutating `Git` binding sang git adapter va tao `RollbackTarget::GitRef`

## HTTP
- initial slice: `prepare` capture method/url/request_digest cho `HttpRequest`
- `HttpRequest.url` hien duoc hieu la bound URL scope/prefix (`base_url + path_prefix`), khong phai luc nao cung la concrete endpoint
- `execute` hien chi support `GET` va capture HTTP status
- `execute` uu tien `payload.url` / `payload.method` neu co; adapter fail-closed neu URL vuot khoi bound scope hoac method khong khop binding
- execute-time metadata phan biet `bound_url` / `executed_url` de verify dung concrete endpoint da chay
- `verify` support `HttpStatusExpected`; neu khong co explicit check thi chi auto-verify cho execute-time status `2xx`
- `rollback` / `compensate` hien la conservative no-op cho `GET`
- gateway chi route mutating HTTP bindings sang adapter; HTTP read-only bindings van di qua enforcement path hien tai
- destructive remote mutation van coi la R3 neu chua co recovery ro
