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
- request-shape digest da body-aware, header-aware, query-aware, va auth-aware: GET = SHA256(method:canonical_url[:headers]), POST/PUT/PATCH/DELETE = SHA256(method:canonical_url:body[:headers]); header names duoc canonicalize lowercase truoc khi hash; query strings duoc canonicalize (sort by key) truoc khi hash de dam bao `?a=1&b=2` va `?b=2&a=1` cung tao ra cung mot digest; bearer/basic/api_key auth token duoc bao gom trong digest khi su dung `auth` field
- dedicated auth representation: `{"auth": {"type": "bearer", "token": "..."}}`, `{"auth": {"type": "basic", "username": "...", "password": "..."}}`, or `{"auth": {"type": "api_key", "header": "X-API-Key", "key": "..."}}`; adapter fail-closed khi malformed auth (token rong, unsupported type); reject ambiguous auth khi ca `headers.authorization` va `auth` deu duoc cung cap; reject ambiguous auth khi api_key header xuat hien trong ca headers va auth.api_key
- auth metadata chi luu presence boolean va digest (SHA256 cua token), khong luu raw token
- `execute` support GET/POST/PUT/PATCH/DELETE; adapter fail-closed neu `payload.url` / `payload.method` vuot bound scope, khong khop binding, hoac khong khop `approved_request_digest`
- execute-time metadata phan biet `bound_url` / `approved_url` / `executed_url`; body, headers, va query duoc luu duoi dang digest thay vi raw values; `approved_query_present` / `executed_query_present` boolean cho presence; `approved_query_digest` / `executed_query_digest` cho query string digest; `approved_auth_present` / `executed_auth_present` va `approved_auth_digest` / `executed_auth_digest` cho auth; `approved_auth_kind` / `executed_auth_kind` chi observability kind (`"bearer"`, `"basic"`, `"api_key"`) khong co secret
- firewall enforce allowlist: khi `auth.bearer` hoac `auth.basic` present, firewall treat nhu co `authorization` header trong allowlist checking; khi `auth.api_key` present, firewall check rang specific api_key header (e.g., `X-API-Key`) nam trong allowlist
- `verify` support `HttpStatusExpected`; GET co the re-request, con mutation methods chi verify bang execute-time metadata va khong replay side effect
- `rollback` / `compensate` la conservative no-op; destructive remote mutation van la explicit R3 boundary cho toi khi co recovery ro rang
- gateway chi route mutating HTTP bindings sang adapter; HTTP read-only bindings van di qua enforcement path hien tai
- **Slice 16-A boundary ratification**: HTTP mutation recovery is explicitly R3/manual; EmailSend is denied at prepare-time. See `docs/implementation-path/16a-slice-16-a-boundary-ratification.md` for entry criteria for any future expansion.
