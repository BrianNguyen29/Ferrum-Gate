# ferrum-adapter-maildraft

Maildraft rollback adapter used for draft-only recovery evidence.

Trang thai hien tai:
- execute tao draft artifact va tra `draft_id`
- verify xac nhan draft artifact van ton tai cho execution do
- rollback/compensate xoa draft artifact da tao
- fail-closed cho `send=true`; send semantics van ngoai supported scope hien tai
