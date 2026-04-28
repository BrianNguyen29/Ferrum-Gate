use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Copies a consistent snapshot of the SQLite database at db_path to destination path.
/// Uses the rusqlite backup API to capture all content including WAL/SHM state.
/// Sets restrictive file permissions (0600) on the destination file.
fn copy_db_snapshot(db_path: &Path, dest_path: &Path) -> Result<u64> {
    // Open source database read-only
    let src = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| {
        format!(
            "failed to open source database '{}' read-only",
            db_path.display()
        )
    })?;

    // Create destination database for backup
    let mut dst = Connection::open(dest_path)
        .with_context(|| format!("failed to create snapshot file '{}'", dest_path.display()))?;

    // Perform backup using rusqlite backup API
    {
        let backup =
            rusqlite::backup::Backup::new(&src, &mut dst).context("failed to initialize backup")?;
        backup
            .run_to_completion(5, std::time::Duration::from_millis(250), None)
            .context("snapshot copy failed — source may be corrupted or busy")?;
    }

    // Set restrictive permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(dest_path, perms)
            .context("failed to set restrictive permissions on snapshot file")?;
    }

    let size = std::fs::metadata(dest_path).map(|m| m.len()).unwrap_or(0);

    Ok(size)
}

/// Creates a backup of the SQLite database at db_path and writes it to output_dir.
/// Uses the rusqlite backup API for a consistent snapshot.
/// Sets restrictive file permissions (0600) on the backup file.
pub fn backup_create(db_path: &Path, output_dir: &Path) -> Result<PathBuf> {
    // Determine backup filename
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let db_name = db_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("backup.db");
    let backup_filename = format!("{}_{}.db", db_name, timestamp);
    let backup_path = output_dir.join(&backup_filename);

    // Create output directory if it doesn't exist
    std::fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "failed to create output directory '{}'",
            output_dir.display()
        )
    })?;

    let size = copy_db_snapshot(db_path, &backup_path)?;

    eprintln!("Backup created: {} ({} bytes)", backup_path.display(), size);

    Ok(backup_path)
}

/// Verifies the integrity of the SQLite database at db_path.
/// Runs PRAGMA integrity_check and returns Ok if the result is "ok".
/// Opens the database read-only; does not create the database.
pub fn backup_verify(db_path: &Path) -> Result<()> {
    let db = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("failed to open database '{}' read-only", db_path.display()))?;

    let result: String = db
        .query_row("PRAGMA integrity_check", [], |row: &rusqlite::Row| {
            row.get(0)
        })
        .context("failed to run integrity_check — database may be corrupted")?;

    if result.trim().eq_ignore_ascii_case("ok") {
        eprintln!("Database integrity check passed: {}", db_path.display());
        Ok(())
    } else {
        bail!(
            "Database integrity check FAILED on '{}': {}",
            db_path.display(),
            result
        );
    }
}

/// Restores a database from a backup file to the original location.
/// Safety guard: requires --confirm flag.
/// Attempts a best-effort exclusive transaction on the current DB before mutation.
/// If the transaction cannot be acquired, restore is refused; operators must still stop ferrumd.
/// Preserves a pre-restore copy before overwriting.
/// Verifies the restored database passes integrity_check.
pub fn backup_restore(db_path: &Path, from_path: &Path, confirm: bool) -> Result<()> {
    if !confirm {
        bail!("--confirm flag is required to restore a database");
    }

    // Validate from_path exists
    if !from_path.exists() {
        bail!("backup file '{}' does not exist", from_path.display());
    }

    // First verify the backup file is valid before touching anything
    {
        let backup_db = Connection::open(from_path)
            .with_context(|| format!("failed to open backup file '{}'", from_path.display()))?;
        let result: String = backup_db
            .query_row("PRAGMA integrity_check", [], |row: &rusqlite::Row| {
                row.get(0)
            })
            .context("backup file failed integrity check — refusing to restore")?;
        if !result.trim().eq_ignore_ascii_case("ok") {
            bail!(
                "Backup file integrity check FAILED: {}. Refusing to restore.",
                result
            );
        }
    }

    // Try a best-effort exclusive transaction on the current db before restore.
    // If the db is locked by another process (server or active writer), this will fail.
    // We check if the db_path exists — if it doesn't, no running server could have it locked.
    let needs_lock_check = db_path.exists();
    if needs_lock_check {
        // Open read-write and attempt BEGIN EXCLUSIVE before any file mutation.
        // This is a safety guard, not a substitute for stopping ferrumd before restore.
        // If another connection holds a lock, this will fail with SQLITE_BUSY.
        match Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(conn) => {
                // Attempt to start an exclusive transaction before mutation.
                let start_result = conn.execute_batch("BEGIN EXCLUSIVE");
                match start_result {
                    Ok(()) => {
                        // Successfully acquired exclusive transaction. Rollback to release it
                        // and allow us to proceed with restore.
                        let _ = conn.execute_batch("ROLLBACK");
                    }
                    Err(rusqlite::Error::SqliteFailure(ref err, _))
                        if err.code == rusqlite::ErrorCode::DatabaseLocked =>
                    {
                        bail!(
                            "Database '{}' appears to be locked (server may be running). \
                             Stop the server before restoring.",
                            db_path.display()
                        );
                    }
                    Err(e) => {
                        // Some other error during transaction start — be conservative and refuse
                        bail!(
                            "Could not acquire exclusive lock on '{}': {}. \
                             Stop the server before restoring.",
                            db_path.display(),
                            e
                        );
                    }
                }
            }
            Err(rusqlite::Error::SqliteFailure(ref err, _))
                if err.code == rusqlite::ErrorCode::DatabaseLocked
                    || err.code == rusqlite::ErrorCode::CannotOpen =>
            {
                bail!(
                    "Database '{}' appears to be locked (server may be running). \
                     Stop the server before restoring.",
                    db_path.display()
                );
            }
            Err(e) => {
                // Some other error — be conservative and refuse
                bail!(
                    "Could not open '{}' read-write: {}. \
                     Stop the server before restoring.",
                    db_path.display(),
                    e
                );
            }
        }
    }

    // Create pre-restore snapshot if current db exists
    if db_path.exists() {
        let pre_restore_path = PathBuf::from(format!("{}.pre_restore", db_path.display()));
        copy_db_snapshot(db_path, &pre_restore_path).with_context(|| {
            format!(
                "failed to create pre-restore snapshot at '{}'",
                pre_restore_path.display()
            )
        })?;
        eprintln!("Pre-restore snapshot saved: {}", pre_restore_path.display());
    }

    // Copy backup to target location
    std::fs::copy(from_path, db_path)
        .with_context(|| format!("failed to copy backup to '{}'", db_path.display()))?;

    // Set restrictive permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(db_path, perms);
    }

    // Verify restored database
    backup_verify(db_path)?;

    eprintln!("Database restored successfully: {}", db_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn create_test_db(path: &Path, sql: &str) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(sql).unwrap();
    }

    #[test]
    fn test_backup_create_and_verify() {
        // Create a temp directory
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backup_dir = temp_dir.path().join("backups");

        // Create a test database with some data
        create_test_db(
            &db_path,
            "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT); \
             INSERT INTO test (name) VALUES ('hello'); \
             INSERT INTO test (name) VALUES ('world');",
        );

        // Verify original db
        backup_verify(&db_path).expect("original db should pass integrity check");

        // Create backup
        let backup_path =
            backup_create(&db_path, &backup_dir).expect("backup create should succeed");
        assert!(backup_path.exists());

        // Verify backup file passes integrity check
        backup_verify(&backup_path).expect("backup should pass integrity check");

        // Verify backup is a different file
        let mut original_content = Vec::new();
        let mut backup_content = Vec::new();
        std::fs::File::open(&db_path)
            .unwrap()
            .read_to_end(&mut original_content)
            .unwrap();
        std::fs::File::open(&backup_path)
            .unwrap()
            .read_to_end(&mut backup_content)
            .unwrap();
        assert_eq!(original_content.len(), backup_content.len());
    }

    #[test]
    fn test_backup_verify_detects_corruption() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("corrupt.db");

        // Create a database
        create_test_db(
            &db_path,
            "CREATE TABLE test (id INTEGER PRIMARY KEY); INSERT INTO test (id) VALUES (1);",
        );

        // Verify it passes initially
        backup_verify(&db_path).expect("fresh db should pass integrity check");

        // Corrupt the database header by overwriting the first few bytes
        // SQLite database header starts with "SQLite format 3\000"
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&db_path)
            .unwrap();
        file.write_all(b"NOT a valid header").unwrap();
        drop(file);

        // Verify should now fail
        let result = backup_verify(&db_path);
        assert!(result.is_err(), "corrupted db should fail integrity check");
    }

    #[test]
    fn test_backup_restore_requires_confirm() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backup_path = temp_dir.path().join("backup.db");

        // Create a backup file
        create_test_db(
            &backup_path,
            "CREATE TABLE test (id INTEGER PRIMARY KEY); INSERT INTO test (id) VALUES (1);",
        );

        // Restore without --confirm should fail
        let result = backup_restore(&db_path, &backup_path, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("--confirm"));
    }

    #[test]
    fn test_backup_restore_preserves_pre_restore_copy() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backup_path = temp_dir.path().join("backup.db");

        // Create original database
        create_test_db(
            &db_path,
            "CREATE TABLE test (id INTEGER PRIMARY KEY, data TEXT); INSERT INTO test (data) VALUES ('original');",
        );

        // Create backup with different data
        create_test_db(
            &backup_path,
            "CREATE TABLE test (id INTEGER PRIMARY KEY, data TEXT); INSERT INTO test (data) VALUES ('restored');",
        );

        let pre_restore_path = PathBuf::from(format!("{}.pre_restore", db_path.display()));

        // Restore with --confirm
        backup_restore(&db_path, &backup_path, true).expect("restore should succeed");

        // Pre-restore copy should exist
        assert!(pre_restore_path.exists(), "pre-restore copy should exist");

        // Verify the pre-restore copy has original data
        let conn = Connection::open(&pre_restore_path).unwrap();
        let data: String = conn
            .query_row("SELECT data FROM test", [], |row: &rusqlite::Row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(data, "original");

        // Verify restored db has backup data
        let conn2 = Connection::open(&db_path).unwrap();
        let data2: String = conn2
            .query_row("SELECT data FROM test", [], |row: &rusqlite::Row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(data2, "restored");
    }

    #[test]
    fn test_backup_restore_verifies_after_restore() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backup_path = temp_dir.path().join("backup.db");

        // Create a valid backup
        create_test_db(
            &backup_path,
            "CREATE TABLE test (id INTEGER PRIMARY KEY); INSERT INTO test (id) VALUES (1);",
        );

        backup_restore(&db_path, &backup_path, true).expect("restore should succeed");

        // Verify the restored db
        backup_verify(&db_path).expect("restored db should pass integrity check");
    }

    #[test]
    fn test_backup_restore_refuses_corrupt_backup() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backup_path = temp_dir.path().join("corrupt_backup.db");

        // Create original database
        create_test_db(&db_path, "CREATE TABLE test (id INTEGER PRIMARY KEY);");

        // Create a "corrupt" backup by just touching a file
        std::fs::write(&backup_path, b"this is not a valid sqlite database").unwrap();

        let result = backup_restore(&db_path, &backup_path, true);
        assert!(result.is_err(), "restore should refuse corrupt backup");
        assert!(
            result.unwrap_err().to_string().contains("integrity check"),
            "error should mention integrity check"
        );
    }

    #[test]
    fn test_backup_restore_refuses_nonexistent_backup() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backup_path = temp_dir.path().join("nonexistent.db");

        // Create original database so db_path exists
        create_test_db(&db_path, "CREATE TABLE test (id INTEGER PRIMARY KEY);");

        let result = backup_restore(&db_path, &backup_path, true);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_backup_restore_refuses_locked_db() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backup_path = temp_dir.path().join("backup.db");

        // Create original database
        create_test_db(
            &db_path,
            "CREATE TABLE test (id INTEGER PRIMARY KEY, data TEXT); INSERT INTO test (data) VALUES ('original');",
        );

        // Create backup with different data
        create_test_db(
            &backup_path,
            "CREATE TABLE test (id INTEGER PRIMARY KEY, data TEXT); INSERT INTO test (data) VALUES ('restored');",
        );

        // Open the database with an exclusive transaction (BEGIN EXCLUSIVE)
        // This simulates a running server holding an exclusive lock
        let locking_conn = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .unwrap();
        locking_conn
            .execute_batch("BEGIN EXCLUSIVE")
            .expect("should be able to begin exclusive transaction");

        // Attempt to restore — should fail because DB is locked
        let result = backup_restore(&db_path, &backup_path, true);
        assert!(
            result.is_err(),
            "restore should refuse when DB is locked by another connection"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("locked") || err_msg.contains("Stop the server"),
            "error should mention locked or stop server: {}",
            err_msg
        );

        let pre_restore_path = PathBuf::from(format!("{}.pre_restore", db_path.display()));
        assert!(
            !pre_restore_path.exists(),
            "pre_restore copy should NOT exist because restore refused before mutation"
        );

        // Drop the exclusive transaction before opening a new connection for content verification.
        locking_conn.execute_batch("ROLLBACK").unwrap();

        // Verify the original DB is completely unchanged.
        let conn = Connection::open(&db_path).unwrap();
        let data: String = conn
            .query_row("SELECT data FROM test", [], |row: &rusqlite::Row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            data, "original",
            "original data should be unchanged after failed restore"
        );
    }
}
