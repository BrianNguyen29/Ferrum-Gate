# ferrum-adapter-sqlite

SQLite rollback adapter used for supported recovery evidence.

Trang thai hien tai:
- execute thuc hien row upsert tren file-backed SQLite DB
- prepare/execute luu metadata can thiet cho verify va recovery
- verify doi chieu row state voi execute-time snapshot
- rollback/compensate khoi phuc row cu hoac xoa row moi tao neu truoc do khong ton tai
