use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use hunk_updater::{AssetFormat, ReleaseAsset, ReleaseManifest, sign_payload};

struct CliArgs {
    version: String,
    base_url: String,
    private_key_base64: String,
    output_dir: PathBuf,
    pub_date: Option<String>,
    notes: Option<String>,
    manifest_name: String,
    assets: Vec<AssetArg>,
}

struct AssetArg {
    target: String,
    format: AssetFormat,
    path: PathBuf,
}

fn main() -> Result<()> {
    let args = parse_args(env::args().skip(1).collect())?;
    fs::create_dir_all(&args.output_dir).with_context(|| {
        format!(
            "failed to create updater manifest output directory {}",
            args.output_dir.display()
        )
    })?;

    let mut platforms = BTreeMap::new();
    for asset in &args.assets {
        let payload = fs::read(&asset.path)
            .with_context(|| format!("failed to read release asset {}", asset.path.display()))?;
        let signature = sign_payload(&payload, args.private_key_base64.as_str())
            .with_context(|| format!("failed to sign release asset {}", asset.path.display()))?;
        let file_name = asset
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| anyhow!("asset path has no file name: {}", asset.path.display()))?;
        let signature_path = args.output_dir.join(format!("{file_name}.sig"));
        fs::write(&signature_path, format!("{signature}\n")).with_context(|| {
            format!(
                "failed to write updater signature file {}",
                signature_path.display()
            )
        })?;

        platforms.insert(
            asset.target.clone(),
            ReleaseAsset {
                url: format!("{}/{}", args.base_url.trim_end_matches('/'), file_name),
                signature,
                format: asset.format,
            },
        );
    }

    let manifest = ReleaseManifest {
        version: args.version,
        pub_date: args.pub_date,
        notes: args.notes,
        platforms,
    };
    let manifest_path = args.output_dir.join(args.manifest_name);
    let manifest_bytes =
        serde_json::to_vec_pretty(&manifest).context("failed to serialize update manifest")?;
    fs::write(&manifest_path, &manifest_bytes)
    .with_context(|| {
        format!(
            "failed to write update manifest {}",
            manifest_path.display()
        )
    })?;
    let manifest_signature = sign_payload(&manifest_bytes, args.private_key_base64.as_str())
        .context("failed to sign update manifest")?;
    let manifest_signature_path = args
        .output_dir
        .join(format!("{}.sig", manifest_path.file_name().and_then(|name| name.to_str()).ok_or_else(
            || anyhow!("manifest path has no file name: {}", manifest_path.display())
        )?));
    fs::write(&manifest_signature_path, format!("{manifest_signature}\n")).with_context(|| {
        format!(
            "failed to write update manifest signature {}",
            manifest_signature_path.display()
        )
    })?;

    println!("{}", manifest_path.display());
    Ok(())
}

fn parse_args(args: Vec<String>) -> Result<CliArgs> {
    let mut version = None;
    let mut base_url = None;
    let mut private_key_base64 = None;
    let mut output_dir = None;
    let mut pub_date = None;
    let mut notes = None;
    let mut notes_file = None;
    let mut manifest_name = "stable.json".to_string();
    let mut assets = Vec::new();

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--version" => version = Some(next_value(&mut iter, "--version")?),
            "--base-url" => base_url = Some(next_value(&mut iter, "--base-url")?),
            "--private-key-base64" => {
                private_key_base64 = Some(next_value(&mut iter, "--private-key-base64")?)
            }
            "--output-dir" => {
                output_dir = Some(PathBuf::from(next_value(&mut iter, "--output-dir")?))
            }
            "--pub-date" => pub_date = Some(next_value(&mut iter, "--pub-date")?),
            "--notes" => notes = Some(next_value(&mut iter, "--notes")?),
            "--notes-file" => {
                notes_file = Some(PathBuf::from(next_value(&mut iter, "--notes-file")?))
            }
            "--manifest-name" => manifest_name = next_value(&mut iter, "--manifest-name")?,
            "--asset" => assets.push(parse_asset_arg(next_value(&mut iter, "--asset")?.as_str())?),
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => bail!("unknown argument `{other}`"),
        }
    }

    if notes.is_some() && notes_file.is_some() {
        bail!("use only one of --notes or --notes-file");
    }

    let notes = match (notes, notes_file) {
        (Some(notes), None) => Some(notes),
        (None, Some(path)) => Some(
            fs::read_to_string(&path)
                .with_context(|| format!("failed to read notes file {}", path.display()))?,
        ),
        (None, None) => None,
        (Some(_), Some(_)) => unreachable!(),
    };

    let version = version.ok_or_else(|| anyhow!("missing required --version"))?;
    let base_url = base_url.ok_or_else(|| anyhow!("missing required --base-url"))?;
    let private_key_base64 =
        private_key_base64.ok_or_else(|| anyhow!("missing required --private-key-base64"))?;
    let output_dir = output_dir.ok_or_else(|| anyhow!("missing required --output-dir"))?;
    if assets.is_empty() {
        bail!("at least one --asset argument is required");
    }

    Ok(CliArgs {
        version,
        base_url,
        private_key_base64,
        output_dir,
        pub_date,
        notes,
        manifest_name,
        assets,
    })
}

fn next_value(iter: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    iter.next()
        .ok_or_else(|| anyhow!("{flag} requires a value"))
}

fn parse_asset_arg(raw: &str) -> Result<AssetArg> {
    let mut parts = raw.splitn(3, ':');
    let target = parts
        .next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("asset spec is missing the platform target"))?;
    let format = parts
        .next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("asset spec is missing the asset format"))?
        .parse::<AssetFormat>()?;
    let path = parts
        .next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("asset spec is missing the asset path"))?;
    if !Path::new(path).exists() {
        bail!("asset path does not exist: {path}");
    }

    Ok(AssetArg {
        target: target.to_string(),
        format,
        path: PathBuf::from(path),
    })
}

fn print_help() {
    eprintln!(
        "Usage: hunk-update-manifest --version <semver> --base-url <url> \
--private-key-base64 <base64> --output-dir <dir> --asset <target:format:path> [--asset ...] \
[--pub-date <iso8601>] [--notes <text> | --notes-file <path>] [--manifest-name <file>]"
    );
}
