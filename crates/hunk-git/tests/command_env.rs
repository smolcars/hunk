use std::env::split_paths;
use std::ffi::OsString;
use std::path::PathBuf;

#[cfg(unix)]
use std::env::join_paths;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;

#[allow(dead_code)]
#[path = "../src/command_env.rs"]
mod command_env;

#[test]
fn macos_gui_search_path_appends_common_signing_dirs_without_duplicates() {
    let search_path = command_env::macos_gui_search_path(
        Some(OsString::from("/usr/bin:/bin:/usr/local/bin")),
        Some(OsString::from("/Users/alice")),
    )
    .expect("expected a merged search path");

    let entries = split_paths(&search_path).collect::<Vec<_>>();

    assert_eq!(entries[0], PathBuf::from("/usr/bin"));
    assert_eq!(entries[1], PathBuf::from("/bin"));
    assert!(entries.contains(&PathBuf::from("/opt/homebrew/bin")));
    assert!(entries.contains(&PathBuf::from("/usr/local/MacGPG2/bin")));
    assert!(entries.contains(&PathBuf::from("/opt/local/bin")));
    assert!(entries.contains(&PathBuf::from("/Users/alice/.local/bin")));
    assert!(entries.contains(&PathBuf::from("/Users/alice/.nix-profile/bin")));
    assert_eq!(
        entries
            .iter()
            .filter(|entry| *entry == &PathBuf::from("/usr/local/bin"))
            .count(),
        1
    );
}

#[test]
fn linux_gui_search_path_appends_common_signing_dirs_without_duplicates() {
    let search_path = command_env::linux_gui_search_path(
        Some(OsString::from("/usr/bin:/bin:/usr/local/bin")),
        Some(OsString::from("/home/alice")),
    )
    .expect("expected a merged search path");

    let entries = split_paths(&search_path).collect::<Vec<_>>();

    assert_eq!(entries[0], PathBuf::from("/usr/bin"));
    assert_eq!(entries[1], PathBuf::from("/bin"));
    assert!(entries.contains(&PathBuf::from("/snap/bin")));
    assert!(entries.contains(&PathBuf::from("/home/linuxbrew/.linuxbrew/bin")));
    assert!(entries.contains(&PathBuf::from("/nix/var/nix/profiles/default/bin")));
    assert!(entries.contains(&PathBuf::from("/run/current-system/sw/bin")));
    assert!(entries.contains(&PathBuf::from("/home/alice/.local/bin")));
    assert!(entries.contains(&PathBuf::from("/home/alice/.linuxbrew/bin")));
    assert!(entries.contains(&PathBuf::from("/home/alice/.nix-profile/bin")));
    assert_eq!(
        entries
            .iter()
            .filter(|entry| *entry == &PathBuf::from("/usr/local/bin"))
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn resolve_program_in_search_path_prefers_first_executable_match() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let first_dir = tempdir.path().join("first");
    let second_dir = tempdir.path().join("second");
    fs::create_dir_all(&first_dir).expect("create first dir");
    fs::create_dir_all(&second_dir).expect("create second dir");

    let first_git = first_dir.join("git");
    let second_git = second_dir.join("git");
    write_executable(&first_git);
    write_executable(&second_git);

    let search_path = join_paths([first_dir.as_path(), second_dir.as_path()]).expect("join path");
    let resolved = command_env::resolve_program_in_search_path("git", search_path.as_os_str())
        .expect("expected to resolve git");

    assert_eq!(resolved, first_git);
}

#[cfg(unix)]
fn write_executable(path: &std::path::Path) {
    fs::write(path, "#!/bin/sh\nexit 0\n").expect("write executable");
    let mut permissions = fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("set permissions");
}
