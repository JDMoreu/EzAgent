//! F0-03: el `rusqlite` empaquetado trae FTS5 (lo usa la memoria del agente).

#[test]
fn bundled_sqlite_has_fts5() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE VIRTUAL TABLE docs USING fts5(body);")
        .unwrap();
    conn.execute("INSERT INTO docs(body) VALUES (?1)", ["memoria del agente"])
        .unwrap();
    let hits: i64 = conn
        .query_row(
            "SELECT count(*) FROM docs WHERE docs MATCH 'memoria'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1);
}

#[test]
fn bundled_sqlite_version_is_reported() {
    let version: String = rusqlite::Connection::open_in_memory()
        .unwrap()
        .query_row("SELECT sqlite_version()", [], |row| row.get(0))
        .unwrap();
    assert!(version.starts_with('3'), "versión inesperada: {version}");
}
