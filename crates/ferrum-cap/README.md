# ferrum-cap

Capability service crate.

Vai trò:
- mint capability leases
- validate TTL / single-use / scope subset
- revoke capability

Trang thai hien tai:
- co `InMemoryCapabilityService` cho tests nho va local wiring
- co `SqliteCapabilityService` cho durable capability state trong supported flow hien tai
- giu fail-closed semantics cho TTL, single-use, revoke, va scope subset validation
- `get()` van co the doc capability `Used` khi can provenance/inspection
