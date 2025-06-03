use mergiraf::resolve_merge_cascading;
use mergiraf::settings::DisplaySettings;
use std::fs;

#[test]
fn error_when_file_has_conflict_markers_but_not_in_index() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_dir = temp_dir.path();
    let file_path = repo_dir.join("conflicted.py");
    let conflict_content = r#"line 1
<<<<<<< ours
line 2 from ours
||||||| base
line 2 from base
=======
line 2 from theirs
>>>>>>> theirs
line 3
"#;
    fs::write(&file_path, conflict_content).unwrap();
    let handle = caplog::get_handle();
    // The file is just present in the working tree so it's not on a conflicted state
    let result = resolve_merge_cascading(
        conflict_content,
        &file_path,
        DisplaySettings::default(),
        None,
        repo_dir,
        None,
    );
    assert!(
        result.is_ok(),
        "Structured resolution errors are not terminal."
    );
    handle.any_msg_contains("File conflicted.py is not in a conflicted state in the index, cannot extract stage for revision");
}
