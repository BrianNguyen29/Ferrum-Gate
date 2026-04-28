use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OpenFlags};
use std::fs;
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

/// Prunes backup files older than `retention_days` from output_dir.
/// Files matching `<db_name>_*.db` pattern with mtime older than retention_days
/// are deleted. The newly created backup (current_backup_path) is never deleted.
fn prune_old_backups(
    output_dir: &Path,
    db_name: &str,
    retention_days: u32,
    current_backup_path: &Path,
) -> Result<usize> {
    let cutoff = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| {
            d.as_secs()
                .saturating_sub((retention_days as u64) * 24 * 60 * 60)
        })
        .unwrap_or(0);

    let mut pruned_count = 0;

    for entry in fs::read_dir(output_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Skip non-regular files
        if !path.is_file() {
            continue;
        }

        // Skip the newly created backup
        if path == current_backup_path {
            continue;
        }

        // Check if filename matches the pattern
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let prefix = format!("{}_", db_name);
        if !filename.starts_with(&prefix) || !filename.ends_with(".db") {
            continue;
        }

        // Check mtime
        if let Ok(metadata) = entry.metadata() {
            if let Ok(mtime) = metadata.modified() {
                let mtime_secs = mtime
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                if mtime_secs < cutoff {
                    fs::remove_file(&path)?;
                    pruned_count += 1;
                    eprintln!("Pruned old backup: {}", path.display());
                }
            }
        }
    }

    Ok(pruned_count)
}

/// Creates a backup of the SQLite database at db_path and writes it to output_dir.
/// Uses the rusqlite backup API for a consistent snapshot.
/// Sets restrictive file permissions (0600) on the backup file.
#[allow(dead_code)]
pub fn backup_create(db_path: &Path, output_dir: &Path) -> Result<PathBuf> {
    backup_create_with_retention(db_path, output_dir, None).map(|(path, _)| path)
}

/// Creates a backup with optional retention pruning.
/// Returns the backup path and number of pruned files if retention_days is Some.
pub fn backup_create_with_retention(
    db_path: &Path,
    output_dir: &Path,
    retention_days: Option<u32>,
) -> Result<(PathBuf, usize)> {
    // Validate retention_days
    if let Some(n) = retention_days {
        if n == 0 {
            bail!("--retention-days must be at least 1 (got 0)");
        }
    }
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

    // Apply retention pruning if requested
    let pruned_count = if let Some(days) = retention_days {
        prune_old_backups(output_dir, db_name, days, &backup_path)?
    } else {
        0
    };

    Ok((backup_path, pruned_count))
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

    // -------------------------------------------------------------------------
    // Backup retention tests
    // -------------------------------------------------------------------------

    /// Sets file mtime to a specific unix timestamp using touch -t
    fn touch_with_mtime(path: &Path, timestamp_secs: u64) {
        // touch -t format: YYYYMMDDhhmm
        // Convert unix timestamp to YYYYMMDDHHMM
        let days = timestamp_secs / 86400;
        let rem = timestamp_secs % 86400;
        let hour = rem / 3600;
        let min = (rem % 3600) / 60;

        // Calculate year/month/day from days since epoch (1970-01-01)
        let mut year = 1970;
        let mut remaining_days = days as i64;
        loop {
            let days_in_year = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                366
            } else {
                365
            };
            if remaining_days < days_in_year {
                break;
            }
            remaining_days -= days_in_year;
            year += 1;
        }

        let month_days = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        } else {
            [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        };

        let mut month = 1;
        for d in month_days.iter() {
            if remaining_days < *d as i64 {
                break;
            }
            remaining_days -= *d as i64;
            month += 1;
        }
        let day = remaining_days + 1;

        let formatted = format!("{:04}{:02}{:02}{:02}{:02}", year, month, day, hour, min);
        std::process::Command::new("touch")
            .args(["-t", &formatted, &path.to_string_lossy()])
            .output()
            .expect("touch should succeed");
    }

    #[test]
    fn test_backup_create_with_retention_prunes_old_backups() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backup_dir = temp_dir.path().join("backups");

        // Create a test database
        create_test_db(
            &db_path,
            "CREATE TABLE test (id INTEGER PRIMARY KEY); INSERT INTO test (id) VALUES (1);",
        );

        // Create output dir
        std::fs::create_dir_all(&backup_dir).unwrap();

        // Create an old backup file manually (touch with old mtime)
        // Note: backup files are named {db_name}_{timestamp}.db, db_name="test.db"
        let old_backup = backup_dir.join("test.db_1000000000.db");
        std::fs::write(&old_backup, b"fake old backup").unwrap();

        // Set the old backup's mtime to 100 days ago using touch -t
        let old_mtime = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - (100 * 24 * 60 * 60);
        touch_with_mtime(&old_backup, old_mtime);

        // Create a recent backup file that should NOT be deleted
        let recent_backup = backup_dir.join("test.db_2000000000.db");
        std::fs::write(&recent_backup, b"fake recent backup").unwrap();

        // Create new backup with retention=30 days
        let (backup_path, pruned) = backup_create_with_retention(&db_path, &backup_dir, Some(30))
            .expect("backup should succeed");

        // Verify the new backup was created
        assert!(backup_path.exists());

        // Verify old backup was pruned
        assert!(!old_backup.exists(), "old backup should be pruned");

        // Verify recent backup was preserved
        assert!(recent_backup.exists(), "recent backup should be preserved");

        // Verify new backup was preserved
        assert!(backup_path.exists(), "new backup should be preserved");

        // Verify pruned count
        assert_eq!(pruned, 1, "should have pruned exactly 1 backup");
    }

    #[test]
    fn test_backup_create_with_retention_does_not_delete_new_backup() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("mydb.db");
        let backup_dir = temp_dir.path().join("backups");

        // Create a test database
        create_test_db(
            &db_path,
            "CREATE TABLE test (id INTEGER PRIMARY KEY); INSERT INTO test (id) VALUES (1);",
        );

        // Create output dir
        std::fs::create_dir_all(&backup_dir).unwrap();

        // Create new backup with retention=1 day
        let (backup_path, pruned) = backup_create_with_retention(&db_path, &backup_dir, Some(1))
            .expect("backup should succeed");

        // Verify the new backup exists
        assert!(backup_path.exists());

        // Verify nothing was pruned (the new backup is never deleted even if mtime is weird)
        assert_eq!(pruned, 0, "should not prune anything on first backup");

        // Verify we can still verify the backup
        backup_verify(&backup_path).expect("new backup should pass verification");
    }

    #[test]
    fn test_backup_create_with_retention_preserves_nonmatching_files() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backup_dir = temp_dir.path().join("backups");

        // Create a test database
        create_test_db(
            &db_path,
            "CREATE TABLE test (id INTEGER PRIMARY KEY); INSERT INTO test (id) VALUES (1);",
        );

        // Create output dir
        std::fs::create_dir_all(&backup_dir).unwrap();

        // Create an old backup with different prefix (should NOT be matched)
        let other_backup = backup_dir.join("otherdb_1000000000.db");
        std::fs::write(&other_backup, b"other db backup").unwrap();

        // Create old backup with same prefix but different suffix
        let old_matching = backup_dir.join("test.db_1000000000.db");
        std::fs::write(&old_matching, b"old test backup").unwrap();
        let old_mtime = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - (100 * 24 * 60 * 60);
        touch_with_mtime(&old_matching, old_mtime);

        // Create new backup with retention=30 days
        let (backup_path, pruned) = backup_create_with_retention(&db_path, &backup_dir, Some(30))
            .expect("backup should succeed");

        // Verify old matching backup was pruned
        assert!(
            !old_matching.exists(),
            "old matching backup should be pruned"
        );

        // Verify other db backup was preserved (doesn't match pattern)
        assert!(
            other_backup.exists(),
            "non-matching backup should be preserved"
        );

        // Verify new backup was preserved
        assert!(backup_path.exists(), "new backup should be preserved");

        assert_eq!(pruned, 1, "should have pruned exactly 1 backup");
    }

    #[test]
    fn test_backup_create_with_retention_zero_is_invalid() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backup_dir = temp_dir.path().join("backups");

        // Create a test database
        create_test_db(
            &db_path,
            "CREATE TABLE test (id INTEGER PRIMARY KEY); INSERT INTO test (id) VALUES (1);",
        );

        // retention=0 should be an error
        let result = backup_create_with_retention(&db_path, &backup_dir, Some(0));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("retention-days must be at least 1"),
            "error should mention retention-days requirement: {}",
            err
        );
    }

    #[test]
    fn test_backup_create_with_retention_none_unchanged_behavior() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backup_dir = temp_dir.path().join("backups");

        // Create a test database
        create_test_db(
            &db_path,
            "CREATE TABLE test (id INTEGER PRIMARY KEY); INSERT INTO test (id) VALUES (1);",
        );

        // Create output dir
        std::fs::create_dir_all(&backup_dir).unwrap();

        // Create an old backup manually
        // Note: backup files are named {db_name}_{timestamp}.db, db_name="test.db"
        let old_backup = backup_dir.join("test.db_1000000000.db");
        std::fs::write(&old_backup, b"old backup").unwrap();

        // retention=None should behave like original backup_create (no pruning)
        let (backup_path, pruned) = backup_create_with_retention(&db_path, &backup_dir, None)
            .expect("backup should succeed");

        // Verify the new backup was created
        assert!(backup_path.exists());

        // Verify old backup was NOT pruned (retention=None means no pruning)
        assert!(
            old_backup.exists(),
            "old backup should NOT be pruned when retention is None"
        );

        // Verify pruned count is 0
        assert_eq!(
            pruned, 0,
            "should not prune anything when retention is None"
        );
    }

    #[test]
    fn test_backup_create_with_retention_skips_non_db_files() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let backup_dir = temp_dir.path().join("backups");

        // Create a test database
        create_test_db(
            &db_path,
            "CREATE TABLE test (id INTEGER PRIMARY KEY); INSERT INTO test (id) VALUES (1);",
        );

        // Create output dir
        std::fs::create_dir_all(&backup_dir).unwrap();

        // Create various non-matching files
        let txt_file = backup_dir.join("readme.txt");
        std::fs::write(&txt_file, b"this is not a backup").unwrap();

        let other_file = backup_dir.join("test.txt");
        std::fs::write(&other_file, b"not a db file").unwrap();

        // Create new backup with retention=1 day
        let (backup_path, pruned) = backup_create_with_retention(&db_path, &backup_dir, Some(1))
            .expect("backup should succeed");

        // Verify new backup exists
        assert!(backup_path.exists());

        // Verify non-db files are untouched
        assert!(txt_file.exists(), "txt file should be untouched");
        assert!(other_file.exists(), "other file should be untouched");

        // Verify nothing was pruned
        assert_eq!(pruned, 0, "should not prune any files");
    }
}
