use graph_engine::{
    application::graph_ledger::{GraphLedgerReadPort, GraphRead},
    compat::Value,
    infrastructure::sqlite::SqliteSession,
};
use rusqlite::Connection;

#[test]
fn sqlite_semantic_reads_filter_run_parent_generation_and_keep_round_order() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch(
        "CREATE TABLE joins(run_id TEXT,join_id TEXT,payload BLOB);
         INSERT INTO joins VALUES('R','J',x'00ff'),('other','excluded',x'01');
         CREATE TABLE review_delegation_requests(request_slot_id TEXT,parent_branch_id TEXT,generation INTEGER,status TEXT,round_number INTEGER);
         CREATE TABLE review_delegation_collections(request_slot_id TEXT,collection_digest TEXT,content_json TEXT);
         INSERT INTO review_delegation_requests VALUES('S2','P',2,'sealed',2),('S1','P',2,'sealed',1),('open','P',2,'running',3),('other-parent','Q',2,'sealed',4),('other-generation','P',1,'sealed',5);
         INSERT INTO review_delegation_collections VALUES('S2','D2','{}'),('S1','D1','{}'),('open','DO','{}'),('other-parent','DP','{}'),('other-generation','DG','{}');",
    ).unwrap();
    let session = SqliteSession::from_connection(connection);
    let rows = session
        .all(GraphRead::JoinsForValidation, &["R".into()])
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0]["join_id"].is_text("J"));
    assert!(matches!(&rows[0]["payload"], Value::Bytes(bytes) if bytes == &[0,255]));
    let mut visited = Vec::new();
    session
        .visit(GraphRead::JoinsForValidation, &["R".into()], &mut |row| {
            visited.push(row);
            Ok(())
        })
        .unwrap();
    assert_eq!(visited.len(), 1);
    assert!(visited[0].py_eq(&rows[0]));
    let collections = session
        .all(
            GraphRead::SealedReviewCollections,
            &["P".into(), 2_i64.into()],
        )
        .unwrap();
    assert_eq!(collections.len(), 2);
    assert!(collections[0]["request_slot_id"].is_text("S1"));
    assert!(collections[0]["collection_digest"].is_text("D1"));
    assert!(collections[1]["request_slot_id"].is_text("S2"));
    assert!(collections[1]["collection_digest"].is_text("D2"));
}

#[test]
fn artifact_kind_digest_projection_does_not_decode_other_columns() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE artifacts(ref TEXT,kind TEXT,sha256 TEXT,unselected TEXT);
         INSERT INTO artifacts VALUES('reference','source','digest',CAST(x'ff' AS TEXT));",
        )
        .unwrap();
    let session = SqliteSession::from_connection(connection);
    let record = session
        .first(GraphRead::ArtifactKindDigest, &["reference".into()])
        .unwrap()
        .unwrap();
    assert!(record["kind"].is_text("source"));
    assert!(record["sha256"].is_text("digest"));
    assert!(record.get("unselected").is_none());
    assert!(session
        .first(GraphRead::Artifact, &["reference".into()])
        .is_err());
}
