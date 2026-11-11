//! What `devtest` needs of the sqlite3 shell, for a phone that has none: SQL from stdin against
//! the database named as the one argument, rows printed with `|` between the columns.
//!
//! Build and push, from this folder, with the NDK's clang for `aarch64-linux-android`:
//!   NDK=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin
//!   CC_aarch64_linux_android=$NDK/aarch64-linux-android31-clang.cmd AR_aarch64_linux_android=$NDK/llvm-ar.exe \
//!   CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$NDK/aarch64-linux-android31-clang.cmd \
//!   cargo build --release --target aarch64-linux-android
//!   adb push target/aarch64-linux-android/release/af_sqlite3 /data/local/tmp/af_sqlite3
//! and run the suite with PHONE_SQLITE=/data/local/tmp/af_sqlite3.

use rusqlite::types::ValueRef;
use rusqlite::{Batch, Connection};
use std::io::Read;
use std::time::Duration;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: af_sqlite3 <database> < sql")?;
    let mut sql = String::new();
    std::io::stdin().read_to_string(&mut sql)?;
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    let mut batch = Batch::new(&conn, &sql);
    while let Some(mut statement) = batch.next()? {
        let columns = statement.column_count();
        let mut rows = statement.raw_query();
        while let Some(row) = rows.next()? {
            let mut line = Vec::with_capacity(columns);
            for i in 0..columns {
                line.push(match row.get_ref(i)? {
                    ValueRef::Null => String::new(),
                    ValueRef::Integer(n) => n.to_string(),
                    ValueRef::Real(x) => x.to_string(),
                    ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
                    ValueRef::Blob(b) => String::from_utf8_lossy(b).into_owned(),
                });
            }
            println!("{}", line.join("|"));
        }
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
