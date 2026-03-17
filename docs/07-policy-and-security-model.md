# 07 — Policy and security model

## 1. Decisions

FerrumGate dùng 5 decisions:
- Allow
- Deny
- Quarantine
- RequireApproval
- AllowDraftOnly

## 2. Hard deny cases
- scope mismatch
- manifest mismatch
- capability invalid
- resource ngoài intent scope

## 3. Quarantine cases
- taint cao
- contradiction mạnh giữa intent và proposal
- output lineage đáng ngờ

## 4. Require approval cases
- risk cao / critical
- R3 action
- external communication
- admin-like mutations

## 5. Draft-only cases
- send/publish path nhưng policy muốn giảm quyền về draft

## 6. Trust labels cần theo dõi
- Trusted
- InternalPolicy
- InternalSystem
- ExternalWeb
- ExternalEmail
- ExternalRepoText
- ExternalToolMetadata
- ExternalToolOutput
- Untrusted

## 7. Những gì FerrumGate không giải quyết toàn bộ
- host compromise
- kernel/OS-level isolation
- toàn bộ prompt injection ở mọi tầng
- plugin chạy cùng trust domain nếu integrator cố bypass design
