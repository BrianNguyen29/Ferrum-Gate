# ferrum-adapter-fs

Filesystem rollback adapter used for supported recovery evidence.

Trang thai hien tai:
- prepare snapshot noi dung file neu target da ton tai
- execute tao hoac overwrite file that
- verify xac nhan target ton tai
- rollback xoa file moi tao khi khong co pre-state
- compensate khoi phuc noi dung goc khi la overwrite flow
