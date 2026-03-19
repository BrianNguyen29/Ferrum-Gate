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
- `prepare` capture bound method/url/request_digest cho `HttpRequest`
- `HttpRequest.url` hien duoc hieu la bound URL scope/prefix (`base_url + path_prefix`), khong phai luc nao cung la concrete endpoint
- gateway truyen approved HTTP proposal args vao `prepare` de adapter tinh `approved_request_digest` tren concrete request da approve
- body-aware digest: GET = SHA256(method:url), POST/PUT/PATCH/DELETE = SHA256(method:url:body)
- `execute` van chi support `GET`; adapter fail-closed neu `payload.url` / `payload.method` vuot bound scope, khong khop binding, hoac khong khop `approved_request_digest`
- execute-time metadata phan biet `bound_url` / `approved_url` / `executed_url`; body duoc luu duoi dang digest thay vi raw body
- `verify` support `HttpStatusExpected`; neu khong co explicit check thi chi auto-verify cho execute-time status `2xx`
- `rollback` / `compensate` la conservative no-op; destructive remote mutation van la explicit R3 boundary cho toi khi co recovery ro rang
- gateway chi route mutating HTTP bindings sang adapter; HTTP read-only bindings van di qua enforcement path hien tai
