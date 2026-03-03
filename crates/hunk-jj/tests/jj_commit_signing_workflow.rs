#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use hunk_jj::jj::commit_staged;

#[test]
fn commit_staged_uses_git_signing_key_when_git_commit_signing_is_enabled() {
    if !ssh_keygen_available() {
        eprintln!("Skipping signing workflow test because ssh-keygen is unavailable.");
        return;
    }

    let fixture = TempRepo::new("commit-signing-workflow");
    configure_repo_identity(fixture.path());

    let signing_key_path = fixture.path().join("test-signing-key");
    generate_ssh_signing_key(&signing_key_path);
    enable_git_commit_signing(fixture.path(), &signing_key_path);

    write_file(fixture.path().join("tracked.txt"), "line one\n");
    commit_staged(fixture.path(), "signed commit from app")
        .expect("commit should succeed with Git signing fallback");

    let signature_state = run_jj_output(
        fixture.path(),
        &[
            "log",
            "-r",
            "@-",
            "--no-graph",
            "-T",
            "if(self.signature(), \"signed\", \"unsigned\")",
        ],
    );
    assert_eq!(
        signature_state.trim(),
        "signed",
        "commit created through app JJ backend should be signed"
    );
}

struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("hunk-{prefix}-{unique}"));
        fs::create_dir_all(&path).expect("temp repo directory should be created");

        run_jj(&path, ["git", "init", "--colocate"]);
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn configure_repo_identity(cwd: &Path) {
    run_jj(
        cwd,
        ["config", "set", "--repo", "user.name", "Hunk Test User"],
    );
    run_jj(
        cwd,
        [
            "config",
            "set",
            "--repo",
            "user.email",
            "hunk-tests@example.com",
        ],
    );
    run_jj(cwd, ["metaedit", "--update-author"]);
}

fn generate_ssh_signing_key(path: &Path) {
    let status = Command::new("ssh-keygen")
        .arg("-q")
        .arg("-t")
        .arg("ed25519")
        .arg("-N")
        .arg("")
        .arg("-f")
        .arg(path)
        .status()
        .expect("ssh-keygen should run");
    assert!(status.success(), "ssh-keygen command failed");
}

fn enable_git_commit_signing(cwd: &Path, signing_key_path: &Path) {
    let config_path = cwd.join(".git").join("config");
    let mut config = fs::read_to_string(&config_path).expect("git config should be readable");
    config.push_str("\n[commit]\n\tgpgSign = true\n");
    config.push_str("[gpg]\n\tformat = ssh\n");
    config.push_str("[user]\n\tsigningKey = ");
    config.push_str(signing_key_path.to_string_lossy().as_ref());
    config.push('\n');
    fs::write(config_path, config).expect("git config should be writable");
}

fn write_file(path: PathBuf, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directories should be created");
    }
    fs::write(path, contents).expect("file should be written");
}

fn ssh_keygen_available() -> bool {
    Command::new("ssh-keygen").arg("-h").output().is_ok()
}

fn run_jj<const N: usize>(cwd: &Path, args: [&str; N]) {
    let status = Command::new("jj")
        .args(args)
        .current_dir(cwd)
        .status()
        .expect("jj command should run");
    assert!(status.success(), "jj command failed");
}

fn run_jj_output(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("jj")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("jj command should run");
    assert!(
        output.status.success(),
        "jj command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("jj command output should be utf-8")
}
