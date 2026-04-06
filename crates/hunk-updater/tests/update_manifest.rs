use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use flate2::Compression;
use flate2::write::GzEncoder;
use hunk_updater::{
    AppliedUpdate, AssetFormat, ReleaseAsset, ReleaseManifest, UpdateCheckResult,
    UpdateInstallTarget, apply_staged_update_from_current_executable, current_update_target,
    detect_install_source, detect_install_target, evaluate_manifest,
    install_source_from_explanation, public_key_from_private_key_base64, sign_payload,
    verify_payload_signature,
};
use tar::Builder;

fn sample_manifest(version: &str) -> ReleaseManifest {
    let mut platforms = BTreeMap::new();
    platforms.insert(
        "macos-aarch64".to_string(),
        ReleaseAsset {
            url: "https://example.com/Hunk.app.tar.gz".to_string(),
            signature: "sig-macos".to_string(),
            format: AssetFormat::App,
        },
    );
    platforms.insert(
        "windows-x86_64".to_string(),
        ReleaseAsset {
            url: "https://example.com/Hunk.msi".to_string(),
            signature: "sig-windows".to_string(),
            format: AssetFormat::Wix,
        },
    );
    platforms.insert(
        "linux-x86_64".to_string(),
        ReleaseAsset {
            url: "https://example.com/Hunk.tar.gz".to_string(),
            signature: "sig-linux".to_string(),
            format: AssetFormat::Tarball,
        },
    );

    ReleaseManifest {
        version: version.to_string(),
        pub_date: Some("2026-04-05T20:00:00Z".to_string()),
        notes: Some("Notes".to_string()),
        platforms,
    }
}

#[test]
fn manifest_update_result_uses_target_asset() {
    let result = evaluate_manifest(
        "https://pub-de32dfa5fe9845849590fa075f3edafa.r2.dev/stable.json",
        "0.0.1",
        "linux-x86_64",
        sample_manifest("0.0.2"),
    )
    .expect("manifest should evaluate");

    match result {
        UpdateCheckResult::UpdateAvailable(update) => {
            assert_eq!(update.version, "0.0.2");
            assert_eq!(update.target, "linux-x86_64");
            assert_eq!(update.asset.signature, "sig-linux");
            assert_eq!(update.asset.format, AssetFormat::Tarball);
        }
        other => panic!("expected update available, got {other:?}"),
    }
}

#[test]
fn manifest_up_to_date_when_remote_is_not_newer() {
    let result = evaluate_manifest(
        "https://pub-de32dfa5fe9845849590fa075f3edafa.r2.dev/stable.json",
        "0.0.2",
        "windows-x86_64",
        sample_manifest("0.0.2"),
    )
    .expect("manifest should evaluate");

    assert_eq!(
        result,
        UpdateCheckResult::UpToDate {
            version: "0.0.2".to_string()
        }
    );
}

#[test]
fn prerelease_manifest_versions_are_rejected() {
    let error = evaluate_manifest(
        "https://pub-de32dfa5fe9845849590fa075f3edafa.r2.dev/stable.json",
        "0.0.1",
        "macos-aarch64",
        sample_manifest("0.0.2-alpha.1"),
    )
    .expect_err("prerelease manifest version should fail");

    assert!(
        error
            .to_string()
            .contains("invalid update manifest version"),
        "unexpected error: {error:#}"
    );
}

#[test]
fn install_source_uses_package_manager_explanation_when_present() {
    let source = install_source_from_explanation(Some(
        "This Hunk install is managed by apt. Update it with apt upgrade.",
    ));

    assert_eq!(
        source.explanation(),
        Some("This Hunk install is managed by apt. Update it with apt upgrade.")
    );
}

#[test]
fn install_source_defaults_to_self_managed_when_explanation_is_missing() {
    assert!(matches!(
        install_source_from_explanation(None),
        hunk_updater::InstallSource::SelfManaged
    ));
}

#[test]
fn install_source_ignores_blank_explanations() {
    assert!(matches!(
        install_source_from_explanation(Some("   ")),
        hunk_updater::InstallSource::SelfManaged
    ));
}

#[test]
fn supported_targets_include_the_current_platform() {
    let target = current_update_target().expect("current platform should be supported");

    assert!(!target.is_empty());
}

#[test]
fn detect_install_source_reads_environment_override() {
    unsafe {
        std::env::set_var(
            hunk_updater::UPDATE_EXPLANATION_ENV_VAR,
            "Managed by package manager",
        );
    }
    let source = detect_install_source();
    unsafe {
        std::env::remove_var(hunk_updater::UPDATE_EXPLANATION_ENV_VAR);
    }

    assert_eq!(source.explanation(), Some("Managed by package manager"));
}

#[test]
fn ed25519_sign_and_verify_round_trip() {
    let private_key_base64 = BASE64_STANDARD.encode([7_u8; 32]);
    let public_key_base64 =
        public_key_from_private_key_base64(private_key_base64.as_str()).expect("public key");
    let signature = sign_payload(b"hunk-update", private_key_base64.as_str()).expect("signature");

    verify_payload_signature(
        b"hunk-update",
        signature.as_str(),
        public_key_base64.as_str(),
    )
    .expect("signature should verify");
}

#[test]
fn signature_verification_rejects_tampered_payloads() {
    let private_key_base64 = BASE64_STANDARD.encode([9_u8; 32]);
    let public_key_base64 =
        public_key_from_private_key_base64(private_key_base64.as_str()).expect("public key");
    let signature = sign_payload(b"hunk-update", private_key_base64.as_str()).expect("signature");

    let error =
        verify_payload_signature(b"tampered", signature.as_str(), public_key_base64.as_str())
            .expect_err("tampered payload should fail verification");

    assert!(
        error.to_string().contains("signature verification failed"),
        "unexpected error: {error:#}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn detect_install_target_resolves_macos_app_bundle() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let current_executable =
        create_fake_macos_app(tempdir.path(), "Current.app", "current-version");

    let install_target =
        detect_install_target(current_executable.as_path()).expect("install target");

    match install_target {
        UpdateInstallTarget::MacOsApp {
            app_path,
            relaunch_executable,
        } => {
            assert_eq!(app_path, tempdir.path().join("Current.app"));
            assert_eq!(relaunch_executable, current_executable);
        }
        other => panic!("expected macOS app install target, got {other:?}"),
    }
}

#[cfg(target_os = "macos")]
#[test]
fn apply_staged_update_replaces_macos_bundle_in_place() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let current_executable = create_fake_macos_app(tempdir.path(), "Hunk.app", "old-version");
    let staged_archive = tempdir.path().join("Hunk.app.tar.gz");
    let replacement_app = tempdir.path().join("replacement").join("Hunk.app");
    create_fake_macos_app_bundle(replacement_app.as_path(), "new-version");
    create_tar_gz_archive(
        staged_archive.as_path(),
        replacement_app.as_path(),
        "Hunk.app",
    );

    let applied_update = apply_staged_update_from_current_executable(
        current_executable.as_path(),
        staged_archive.as_path(),
        AssetFormat::App,
    )
    .expect("apply staged update");

    assert_eq!(
        fs::read_to_string(
            tempdir
                .path()
                .join("Hunk.app")
                .join("Contents")
                .join("Resources")
                .join("version.txt"),
        )
        .expect("version file"),
        "new-version",
    );
    assert_eq!(
        applied_update,
        AppliedUpdate {
            relaunch_executable: tempdir
                .path()
                .join("Hunk.app")
                .join("Contents")
                .join("MacOS")
                .join("hunk_desktop"),
        }
    );
}

#[cfg(target_os = "macos")]
fn create_fake_macos_app(root: &Path, app_name: &str, version: &str) -> PathBuf {
    let app_path = root.join(app_name);
    create_fake_macos_app_bundle(app_path.as_path(), version);
    app_path.join("Contents").join("MacOS").join("hunk_desktop")
}

#[cfg(target_os = "macos")]
fn create_fake_macos_app_bundle(app_path: &Path, version: &str) {
    let executable_path = app_path.join("Contents").join("MacOS").join("hunk_desktop");
    let resource_path = app_path
        .join("Contents")
        .join("Resources")
        .join("version.txt");
    fs::create_dir_all(executable_path.parent().expect("executable parent"))
        .expect("create executable directory");
    fs::create_dir_all(resource_path.parent().expect("resource parent"))
        .expect("create resource directory");
    fs::write(executable_path, b"#!/bin/sh\nexit 0\n").expect("write executable");
    fs::write(resource_path, version).expect("write version resource");
}

#[cfg(target_os = "macos")]
fn create_tar_gz_archive(archive_path: &Path, source_path: &Path, archive_name: &str) {
    let file = fs::File::create(archive_path).expect("create archive");
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(encoder);
    builder
        .append_dir_all(archive_name, source_path)
        .expect("append app bundle");
    let encoder = builder.into_inner().expect("finalize tar");
    encoder.finish().expect("finalize gzip");
}
