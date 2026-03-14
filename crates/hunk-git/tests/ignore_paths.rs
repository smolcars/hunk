use std::fs;

use anyhow::Result;
use git2::Repository;
use hunk_git::git::filter_non_ignored_repo_paths;
use tempfile::TempDir;

#[test]
fn filter_non_ignored_repo_paths_honors_gitignore_rules() -> Result<()> {
    let tempdir = TempDir::new()?;
    let repo = Repository::init(tempdir.path())?;

    fs::write(
        tempdir.path().join(".gitignore"),
        "target-shared/\n*.log\n!keep.log\n",
    )?;
    fs::create_dir_all(tempdir.path().join("target-shared"))?;
    fs::create_dir_all(tempdir.path().join("src"))?;
    fs::write(tempdir.path().join("target-shared/output.o"), "artifact\n")?;
    fs::write(tempdir.path().join("src/lib.rs"), "fn main() {}\n")?;
    fs::write(tempdir.path().join("error.log"), "ignored\n")?;
    fs::write(tempdir.path().join("keep.log"), "kept\n")?;

    let included = filter_non_ignored_repo_paths(
        repo.workdir().expect("repo workdir"),
        &[
            ("target-shared".to_string(), true),
            ("target-shared/output.o".to_string(), false),
            ("src/lib.rs".to_string(), false),
            ("error.log".to_string(), false),
            ("keep.log".to_string(), false),
        ],
    )?;

    assert!(!included.contains("target-shared"));
    assert!(!included.contains("target-shared/output.o"));
    assert!(!included.contains("error.log"));
    assert!(included.contains("src/lib.rs"));
    assert!(included.contains("keep.log"));

    Ok(())
}
