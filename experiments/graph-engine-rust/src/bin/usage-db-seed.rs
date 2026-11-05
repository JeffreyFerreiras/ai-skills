#[path = "../../tests/support/store.rs"]
mod store_fixture;

use graph_engine::{
    application::persistent_store::StateStore,
    compat::{self, SerializationProfile, Value},
    domain::ids,
    infrastructure::{
        sqlite::SqliteSession, state_database::NativeStateDatabase,
        state_platform::NativeStatePlatform,
    },
};
use std::{fs, io, path::PathBuf};

fn main() -> io::Result<()> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or_else(|| io::Error::other("output directory required"))?,
    );
    let artifact = fs::canonicalize(
        std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT")
            .ok_or_else(|| io::Error::other("ART required"))?,
    )?;
    if !fs::canonicalize(
        output
            .parent()
            .ok_or_else(|| io::Error::other("output parent required"))?,
    )?
    .starts_with(&artifact)
    {
        return Err(io::Error::other("seed output escaped ART"));
    }
    fs::create_dir(&output)?;
    let fixture = store_fixture::Fixture::new("usage-db-seed");
    let platform = NativeStatePlatform;
    let database = NativeStateDatabase;
    let fault = |_: &str| Ok(());
    let store = StateStore::at_root(fixture.root.join("state"), &platform, &database, &fault)
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    let response = store
        .initialize(&fixture.request())
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    let source = store.db_path("fixture-repository", "RUN-1");
    if !fs::canonicalize(&source)?.starts_with(&artifact) {
        return Err(io::Error::other("seed source escaped ART"));
    }
    let checkpoint = {
        let connection = SqliteSession::connect_existing(&source)
            .map_err(|error| io::Error::other(format!("{error:?}")))?;
        connection
            .query_values("PRAGMA wal_checkpoint(TRUNCATE)", &[])
            .map_err(|error| io::Error::other(format!("{error:?}")))?
    };
    let bytes = fs::read(&source)?;
    let seed = output.join("seed.sqlite3");
    fs::write(&seed, &bytes)?;
    let report = Value::object([
        ("seed_path", seed.to_string_lossy().as_ref().into()),
        ("seed_sha256", ids::sha256_bytes(&bytes).into()),
        ("source_database", source.to_string_lossy().as_ref().into()),
        ("initialization_response", response),
        ("wal_checkpoint", Value::list(checkpoint)),
        ("sqlite_version", graph_engine::infrastructure::sqlite::sqlite_version().into()),
        ("proof_limit", "component seed; copied run inode metadata refers to original initialized DB; direct usage APIs do not claim open_run/core-semantic/full-fixture proof".into()),
    ]);
    fs::write(
        output.join("report.json"),
        compat::dumps(&report, SerializationProfile::SORTED_STORED)
            .map_err(|error| io::Error::other(format!("{error:?}")))?,
    )?;
    println!("{{\"seed_sha256\":\"{}\"}}", ids::sha256_bytes(&bytes));
    Ok(())
}
