//! Temporary on-device probe: proves SQLite (rusqlite `bundled`) runs on the
//! ESP32-P4 against the LittleFS `workspace` partition. Flash it, watch UART
//! for `SQLITE-PROBE:` lines, then reflash the real firmware. Not a product
//! binary — delete after the spike.
//!
//! What it exercises, in order:
//!   1. mount /workspace (same LittleFS partition the product uses);
//!   2. open a database with the `unix-none` VFS (no fcntl locking — the one
//!      POSIX corner ESP-IDF's newlib does not promise) + device pragmas
//!      (journal_mode=TRUNCATE, synchronous=NORMAL);
//!   3. schema DDL, one day of 5-minute samples (288 rows) inside a single
//!      transaction — the Robinhood history write shape;
//!   4. aggregate + chart-window reads;
//!   5. close, reopen, recount — persistence across connections; boot counter
//!      persists across power cycles.

use std::time::Instant;

use esp_idf_svc::fs::littlefs::Littlefs;
use esp_idf_svc::io::vfs::MountedLittlefs;
use rusqlite::{Connection, OpenFlags};

const WORKSPACE_ROOT: &str = "/workspace";
const DB_PATH: &str = "/workspace/sqlite-probe.db";

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    if let Err(error) = run() {
        log::error!("SQLITE-PROBE: FAIL: {error:#}");
    }
    loop {
        std::thread::sleep(std::time::Duration::from_secs(10));
        log::info!("SQLITE-PROBE: idle");
    }
}

fn run() -> anyhow::Result<()> {
    log::info!(
        "SQLITE-PROBE: boot; sqlite version {}",
        rusqlite::version()
    );
    let _mount = mount_workspace()?;
    let heap_before = unsafe { esp_idf_svc::sys::esp_get_free_heap_size() };

    let opened_at = Instant::now();
    let conn = Connection::open_with_flags_and_vfs(
        DB_PATH,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        "unix-none",
    )?;
    log::info!("SQLITE-PROBE: open {:?}", opened_at.elapsed());

    let journal: String =
        conn.query_row("PRAGMA journal_mode=TRUNCATE", [], |row| row.get(0))?;
    conn.execute_batch("PRAGMA synchronous=NORMAL; PRAGMA cache_size=-32;")?;
    log::info!("SQLITE-PROBE: journal_mode={journal}");

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS boots (
             boot_index INTEGER PRIMARY KEY,
             heap_free  INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS samples (
             captured_at    INTEGER PRIMARY KEY,
             total_cents    INTEGER NOT NULL,
             day_pnl_cents  INTEGER NOT NULL
         );",
    )?;

    let boot_index: i64 =
        conn.query_row("SELECT COUNT(*) FROM boots", [], |row| row.get(0))?;
    let prior_rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM samples", [], |row| row.get(0))?;
    conn.execute(
        "INSERT INTO boots (boot_index, heap_free) VALUES (?1, ?2)",
        (boot_index, heap_before as i64),
    )?;
    log::info!("SQLITE-PROBE: boot #{boot_index}, prior samples {prior_rows}");

    // One day of 5-minute portfolio snapshots, one transaction — the write
    // shape the Robinhood history job will produce.
    let wrote_at = Instant::now();
    let base = prior_rows * 300;
    let tx = conn.unchecked_transaction()?;
    {
        let mut insert = tx.prepare(
            "INSERT INTO samples (captured_at, total_cents, day_pnl_cents)
             VALUES (?1, ?2, ?3)",
        )?;
        for i in 0..288i64 {
            let captured_at = base + i * 300;
            let total_cents = 1_500_000 + (i % 97) * 137;
            let day_pnl_cents = (i % 41) - 20;
            insert.execute((captured_at, total_cents, day_pnl_cents))?;
        }
    }
    tx.commit()?;
    log::info!("SQLITE-PROBE: insert 288 rows {:?}", wrote_at.elapsed());

    let read_at = Instant::now();
    let (count, avg_total, max_pnl): (i64, f64, i64) = conn.query_row(
        "SELECT COUNT(*), AVG(total_cents), MAX(day_pnl_cents) FROM samples",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let mut chart = conn.prepare(
        "SELECT captured_at, total_cents FROM samples
         ORDER BY captured_at DESC LIMIT 24",
    )?;
    let window: Vec<(i64, i64)> = chart
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    log::info!(
        "SQLITE-PROBE: aggregate count={count} avg={avg_total:.1} max_pnl={max_pnl}, \
         chart window {} rows, reads {:?}",
        window.len(),
        read_at.elapsed()
    );
    drop(chart);

    // Close and reopen: the rows must come back from flash, not from cache.
    drop(conn);
    let reopened_at = Instant::now();
    let conn = Connection::open_with_flags_and_vfs(
        DB_PATH,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        "unix-none",
    )?;
    let recount: i64 =
        conn.query_row("SELECT COUNT(*) FROM samples", [], |row| row.get(0))?;
    log::info!(
        "SQLITE-PROBE: reopen {:?}, recount {recount}",
        reopened_at.elapsed()
    );
    anyhow::ensure!(
        recount == prior_rows + 288,
        "expected {} rows after reopen, found {recount}",
        prior_rows + 288
    );

    let file_bytes = std::fs::metadata(DB_PATH)?.len();
    let heap_after = unsafe { esp_idf_svc::sys::esp_get_free_heap_size() };
    log::info!(
        "SQLITE-PROBE: db file {file_bytes} bytes, heap {heap_before} -> {heap_after} \
         (delta {})",
        heap_before as i64 - heap_after as i64
    );
    log::info!(
        "SQLITE-PROBE: PASS (boot #{boot_index}, samples {recount})"
    );
    Ok(())
}

// --------------------------------------------------------------------------
// POSIX shims: SQLite's os_unix syscall table takes the ADDRESS of these six
// functions, so the symbols must exist at link time even though our
// configuration (unix-none VFS, lstat=stat, no symlinks, no dotlock files)
// never calls the first five. LittleFS has no users, permissions or
// symlinks, so no-op successes are the honest implementations. nanosleep IS
// called (busy-handler sleeps); route it through ESP-IDF's usleep.
// --------------------------------------------------------------------------

#[repr(C)]
struct Timespec {
    tv_sec: i64, // espidf_time64: 64-bit time_t
    tv_nsec: i32,
}

unsafe extern "C" {
    fn usleep(microseconds: u32) -> i32;
}

#[unsafe(no_mangle)]
extern "C" fn geteuid() -> u32 {
    0
}

#[unsafe(no_mangle)]
extern "C" fn fchmod(_fd: i32, _mode: u32) -> i32 {
    0
}

#[unsafe(no_mangle)]
extern "C" fn fchown(_fd: i32, _owner: u32, _group: u32) -> i32 {
    0
}

#[unsafe(no_mangle)]
extern "C" fn utimes(_path: *const core::ffi::c_char, _times: *const core::ffi::c_void) -> i32 {
    0
}

#[unsafe(no_mangle)]
extern "C" fn readlink(
    _path: *const core::ffi::c_char,
    _buf: *mut core::ffi::c_char,
    _len: usize,
) -> isize {
    -1 // never a symlink on LittleFS
}

#[unsafe(no_mangle)]
extern "C" fn nanosleep(request: *const Timespec, _remain: *mut Timespec) -> i32 {
    let request = unsafe { &*request };
    let micros = (request.tv_sec as u64)
        .saturating_mul(1_000_000)
        .saturating_add((request.tv_nsec as u64) / 1_000);
    unsafe { usleep(micros.min(u32::MAX as u64) as u32) }
}

// Copied from src/storage.rs — the probe is a standalone bin and the package
// has no lib target; keep byte-identical semantics (format only a blank
// partition, never a corrupted one).
type WorkspaceMount = MountedLittlefs<Littlefs<()>>;

fn mount_workspace() -> anyhow::Result<WorkspaceMount> {
    let fs = unsafe { Littlefs::<()>::new_partition("workspace")? };
    match MountedLittlefs::mount(fs, WORKSPACE_ROOT) {
        Ok(mounted) => Ok(mounted),
        Err(_mount_error) if partition_is_blank()? => {
            let mut fs = unsafe { Littlefs::<()>::new_partition("workspace")? };
            fs.format()?;
            MountedLittlefs::mount(fs, WORKSPACE_ROOT).map_err(Into::into)
        }
        Err(mount_error) => Err(anyhow::anyhow!(
            "LittleFS workspace mount failed; preserving non-blank partition: {mount_error}"
        )),
    }
}

fn partition_is_blank() -> anyhow::Result<bool> {
    let partition = unsafe {
        esp_idf_svc::sys::esp_partition_find_first(
            esp_idf_svc::sys::esp_partition_type_t_ESP_PARTITION_TYPE_DATA,
            esp_idf_svc::sys::esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_DATA_LITTLEFS,
            c"workspace".as_ptr(),
        )
    };
    if partition.is_null() {
        anyhow::bail!("LittleFS workspace partition is missing");
    }
    let mut prefix = [0u8; 4096];
    let status = unsafe {
        esp_idf_svc::sys::esp_partition_read(partition, 0, prefix.as_mut_ptr().cast(), prefix.len())
    };
    if status != esp_idf_svc::sys::ESP_OK {
        anyhow::bail!("read LittleFS workspace partition: ESP error {status}");
    }
    Ok(prefix.iter().all(|byte| *byte == 0xff))
}
