# ferrum-firewall

Semantic firewall crate.

Mục tiêu:
- label trust / taint
- sanitize output
- DLP checks
- contradiction checks giữa intent và proposal

Trang thai hien tai:
- co default firewall implementation voi trust labeling, taint scoring, sanitize, va DLP findings
- co contradiction checks cho governance path
- co execution-time payload enforcement cho `File`, `Http`, `Sqlite`, `Git`, va `EmailDraft` bindings
- duoc wire vao supported gateway flow va co regression coverage o integration tests
