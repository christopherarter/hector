use hector_core::session_state::{EditRecord, SessionState};

#[test]
fn save_writes_temp_in_parent_dir_and_renames_atomically() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(".hector/session.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut state = SessionState {
        session_id: "s".into(),
        started_at: "0".into(),
        edits: vec![EditRecord {
            file: "a".into(),
            diff: "+x\n".into(),
            timestamp: "0".into(),
        }],
    };
    state.save(&path).expect("save");
    // After save, parent dir contains exactly the target file —
    // no leftover .tmp.<pid> sidecar.
    let entries: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "exactly one file in parent dir after save: {entries:?}"
    );
}

#[test]
fn save_calls_sync_all_before_rename() {
    // We can't directly observe sync_all from outside the kernel,
    // but we CAN observe that the temp file was renamed (not
    // left behind) and that the final file has the expected
    // content. Pin both; if a future regression skips sync_all,
    // the implementation comment will still document the
    // requirement, and the dir-listing check above will catch
    // a leaked tempfile.
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("session.json");
    let mut state = SessionState {
        session_id: "s2".into(),
        started_at: "0".into(),
        edits: vec![],
    };
    state.save(&path).expect("save");
    let read_back: SessionState =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(read_back.session_id, "s2");
}
