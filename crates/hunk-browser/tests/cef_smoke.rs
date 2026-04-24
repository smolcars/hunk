use std::path::PathBuf;
use std::process::Command;

#[test]
#[ignore = "requires macOS and a staged CEF runtime"]
fn macos_cef_smoke_loads_example_and_paints_nonblank_frame() {
    if !cfg!(target_os = "macos") {
        eprintln!("skipping macOS-only CEF smoke test");
        return;
    }

    let root = workspace_root();
    let runtime_dir = root.join("assets/browser-runtime/cef/macos/runtime");
    if !runtime_dir.join("archive.json").is_file() {
        panic!(
            "staged CEF runtime is missing at {}; run scripts/smoke_browser_cef_macos.sh once",
            runtime_dir.display()
        );
    }

    let output = Command::new(root.join("scripts/smoke_browser_cef_macos.sh"))
        .current_dir(&root)
        .env("CARGO_HOME", "/Volumes/hulk/dev/cache/cargo")
        .env("HUNK_CEF_SKIP_EXPORT", "1")
        .env("HUNK_CEF_SMOKE_RUN_SECONDS", "0")
        .output()
        .expect("CEF smoke script should launch");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "CEF smoke failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("CEF smoke produced nonblank frame")
            || stderr.contains("CEF smoke produced nonblank frame"),
        "CEF smoke did not report a nonblank frame\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root should resolve")
}
