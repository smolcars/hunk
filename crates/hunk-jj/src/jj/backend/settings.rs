fn load_user_settings(workspace_root: Option<&Path>) -> Result<UserSettings> {
    let mut config = StackedConfig::with_defaults();

    if let Some(home_dir) = dirs::home_dir() {
        load_config_if_exists(
            &mut config,
            ConfigSource::User,
            home_dir.join(".jjconfig.toml"),
        )?;
    }

    if let Some(config_dir) = dirs::config_dir() {
        load_config_if_exists(
            &mut config,
            ConfigSource::User,
            config_dir.join("jj").join("config.toml"),
        )?;
    }

    if let Some(root) = workspace_root {
        load_config_if_exists(
            &mut config,
            ConfigSource::Repo,
            root.join(".jj").join("repo").join("config.toml"),
        )?;
        load_config_if_exists(
            &mut config,
            ConfigSource::Workspace,
            root.join(".jj").join("config.toml"),
        )?;
        add_git_signing_fallback_config(&mut config, root)?;
        add_git_identity_fallback_config(&mut config, root)?;
    }

    UserSettings::from_config(config).context("failed to load jj settings")
}

fn add_git_signing_fallback_config(
    config: &mut StackedConfig,
    workspace_root: &Path,
) -> Result<()> {
    if has_explicit_signing_backend(config) {
        return Ok(());
    }

    let Some(git_signing) = read_git_signing_config(workspace_root) else {
        return Ok(());
    };
    let commit_gpgsign = git_signing.commit_gpgsign.unwrap_or(false);
    let git_signing_key = git_signing.signing_key.clone();

    if !commit_gpgsign && git_signing_key.is_none() {
        return Ok(());
    }

    let signing_backend = match git_signing.gpg_format.as_deref() {
        Some("ssh") => "ssh",
        Some("x509") => "gpgsm",
        _ => "gpg",
    };

    let mut fallback_layer = ConfigLayer::empty(ConfigSource::EnvBase);
    fallback_layer
        .set_value("signing.backend", signing_backend)
        .context("failed to apply Git signing backend fallback")?;
    if commit_gpgsign {
        fallback_layer
            .set_value("signing.behavior", "own")
            .context("failed to apply Git commit signing behavior fallback")?;
    }
    if let Some(signing_key) = git_signing_key {
        fallback_layer
            .set_value("signing.key", signing_key)
            .context("failed to apply Git signing key fallback")?;
    }
    if let Some(program) = git_signing.program_for_backend(signing_backend) {
        let key = match signing_backend {
            "ssh" => "signing.backends.ssh.program",
            "gpgsm" => "signing.backends.gpgsm.program",
            _ => "signing.backends.gpg.program",
        };
        fallback_layer
            .set_value(key, program)
            .context("failed to apply Git signing program fallback")?;
    }

    config.add_layer(fallback_layer);
    Ok(())
}

fn add_git_identity_fallback_config(config: &mut StackedConfig, workspace_root: &Path) -> Result<()> {
    let has_user_name = has_explicit_user_name(config);
    let has_user_email = has_explicit_user_email(config);
    if has_user_name && has_user_email {
        return Ok(());
    }

    let Some(identity) = read_git_identity_config(workspace_root) else {
        return Ok(());
    };

    let mut fallback_layer = ConfigLayer::empty(ConfigSource::EnvBase);
    let mut has_updates = false;

    if !has_user_name
        && let Some(name) = identity.name
        && !name.trim().is_empty()
    {
        fallback_layer
            .set_value("user.name", name)
            .context("failed to apply Git user.name fallback")?;
        has_updates = true;
    }

    if !has_user_email
        && let Some(email) = identity.email
        && !email.trim().is_empty()
    {
        fallback_layer
            .set_value("user.email", email)
            .context("failed to apply Git user.email fallback")?;
        has_updates = true;
    }

    if has_updates {
        config.add_layer(fallback_layer);
    }
    Ok(())
}

fn has_explicit_signing_backend(config: &StackedConfig) -> bool {
    config.layers().iter().any(|layer| {
        layer.source != ConfigSource::Default
            && matches!(layer.look_up_item("signing.backend"), Ok(Some(_)))
    })
}

fn has_explicit_user_name(config: &StackedConfig) -> bool {
    config.layers().iter().any(|layer| {
        layer.source != ConfigSource::Default
            && matches!(layer.look_up_item("user.name"), Ok(Some(_)))
    })
}

fn has_explicit_user_email(config: &StackedConfig) -> bool {
    config.layers().iter().any(|layer| {
        layer.source != ConfigSource::Default
            && matches!(layer.look_up_item("user.email"), Ok(Some(_)))
    })
}

#[derive(Default, Clone)]
struct GitSigningConfig {
    commit_gpgsign: Option<bool>,
    signing_key: Option<String>,
    gpg_format: Option<String>,
    gpg_program: Option<String>,
    gpg_ssh_program: Option<String>,
    gpg_x509_program: Option<String>,
}

impl GitSigningConfig {
    fn program_for_backend(&self, backend: &str) -> Option<String> {
        match backend {
            "ssh" => self.gpg_ssh_program.clone(),
            "gpgsm" => self.gpg_x509_program.clone(),
            _ => self.gpg_program.clone(),
        }
    }
}

#[derive(Default, Clone)]
struct GitIdentityConfig {
    name: Option<String>,
    email: Option<String>,
}

fn read_git_signing_config(workspace_root: &Path) -> Option<GitSigningConfig> {
    let mut merged = GitSigningConfig::default();
    let mut saw_any = false;
    let mut visited_paths = BTreeSet::new();

    for path in git_signing_config_paths(workspace_root) {
        if merge_git_signing_config_file(
            &mut merged,
            path.as_path(),
            workspace_root,
            &mut visited_paths,
        ) {
            saw_any = true;
        }
    }

    if saw_any { Some(merged) } else { None }
}

fn read_git_identity_config(workspace_root: &Path) -> Option<GitIdentityConfig> {
    let mut merged = GitIdentityConfig::default();
    let mut saw_any = false;
    let mut visited_paths = BTreeSet::new();

    for path in git_signing_config_paths(workspace_root) {
        if merge_git_identity_config_file(
            &mut merged,
            path.as_path(),
            workspace_root,
            &mut visited_paths,
        ) {
            saw_any = true;
        }
    }

    if saw_any { Some(merged) } else { None }
}

fn git_signing_config_paths(workspace_root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(home_dir) = dirs::home_dir() {
        paths.push(home_dir.join(".gitconfig"));
    }

    let xdg_config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|path| path.join(".config")));
    if let Some(config_home) = xdg_config_home {
        paths.push(config_home.join("git").join("config"));
    }

    if let Some(path) = workspace_git_config_path(workspace_root)
        && !paths.contains(&path)
    {
        paths.push(path);
    }
    if let Some(path) = git_target_config_path(workspace_root)
        && !paths.contains(&path)
    {
        paths.push(path);
    }

    paths
}

fn workspace_git_config_path(workspace_root: &Path) -> Option<PathBuf> {
    workspace_git_dir_path(workspace_root).map(|git_dir| git_dir.join("config"))
}

fn git_target_config_path(workspace_root: &Path) -> Option<PathBuf> {
    let store_root = workspace_root.join(".jj").join("repo").join("store");
    let git_target_path = store_root.join("git_target");
    let raw_target = fs::read_to_string(&git_target_path).ok()?;
    let target = raw_target.trim();
    if target.is_empty() {
        return None;
    }

    let git_repo_path = {
        let target_path = PathBuf::from(target);
        if target_path.is_absolute() {
            target_path
        } else {
            store_root.join(target_path)
        }
    };
    Some(git_repo_path.join("config"))
}

fn workspace_git_dir_path(workspace_root: &Path) -> Option<PathBuf> {
    let dot_git = workspace_root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    if dot_git.is_file() {
        let git_dir = fs::read_to_string(&dot_git).ok().and_then(|contents| {
            contents
                .lines()
                .find_map(|line| line.trim().strip_prefix("gitdir:"))
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
        })?;
        return Some(if git_dir.is_absolute() {
            git_dir
        } else {
            workspace_root.join(git_dir)
        });
    }

    git_target_config_path(workspace_root).and_then(|config_path| config_path.parent().map(Path::to_path_buf))
}

fn merge_git_signing_config_file(
    config: &mut GitSigningConfig,
    path: &Path,
    workspace_root: &Path,
    visited_paths: &mut BTreeSet<PathBuf>,
) -> bool {
    let canonical_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !visited_paths.insert(canonical_path) {
        return false;
    }

    let Ok(contents) = fs::read_to_string(path) else {
        return false;
    };

    let mut saw_any = false;
    let mut section = String::new();
    let mut subsection = None::<String>;

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let header = &line[1..line.len() - 1];
            let (name, sub) = parse_git_config_section_header(header);
            section = name;
            subsection = sub;
            continue;
        }

        let (key, value) = if let Some((key, value)) = line.split_once('=') {
            (
                key.trim().to_ascii_lowercase(),
                normalize_git_config_value(value),
            )
        } else {
            (line.to_ascii_lowercase(), "true".to_string())
        };
        if key.is_empty() {
            continue;
        }

        if key == "path"
            && let Some(include_path) = git_include_path_for_entry(
                section.as_str(),
                subsection.as_deref(),
                value.as_str(),
                path,
                workspace_root,
            )
        {
            if merge_git_signing_config_file(
                config,
                include_path.as_path(),
                workspace_root,
                visited_paths,
            ) {
                saw_any = true;
            }
            continue;
        }

        match (section.as_str(), subsection.as_deref(), key.as_str()) {
            ("commit", None, "gpgsign") => {
                if let Some(value) = parse_git_config_bool(value.as_str()) {
                    config.commit_gpgsign = Some(value);
                    saw_any = true;
                }
            }
            ("user", None, "signingkey") => {
                if !value.is_empty() {
                    config.signing_key = Some(value);
                    saw_any = true;
                }
            }
            ("gpg", None, "format") => {
                if !value.is_empty() {
                    config.gpg_format = Some(value.to_ascii_lowercase());
                    saw_any = true;
                }
            }
            ("gpg", None, "program") => {
                if !value.is_empty() {
                    config.gpg_program = Some(value);
                    saw_any = true;
                }
            }
            ("gpg", Some("ssh"), "program") => {
                if !value.is_empty() {
                    config.gpg_ssh_program = Some(value);
                    saw_any = true;
                }
            }
            ("gpg", Some("x509"), "program") => {
                if !value.is_empty() {
                    config.gpg_x509_program = Some(value);
                    saw_any = true;
                }
            }
            _ => {}
        }
    }

    saw_any
}

fn merge_git_identity_config_file(
    config: &mut GitIdentityConfig,
    path: &Path,
    workspace_root: &Path,
    visited_paths: &mut BTreeSet<PathBuf>,
) -> bool {
    let canonical_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !visited_paths.insert(canonical_path) {
        return false;
    }

    let Ok(contents) = fs::read_to_string(path) else {
        return false;
    };

    let mut saw_any = false;
    let mut section = String::new();
    let mut subsection = None::<String>;

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let header = &line[1..line.len() - 1];
            let (name, sub) = parse_git_config_section_header(header);
            section = name;
            subsection = sub;
            continue;
        }

        let (key, value) = if let Some((key, value)) = line.split_once('=') {
            (
                key.trim().to_ascii_lowercase(),
                normalize_git_config_value(value),
            )
        } else {
            (line.to_ascii_lowercase(), "true".to_string())
        };
        if key.is_empty() {
            continue;
        }

        if key == "path"
            && let Some(include_path) = git_include_path_for_entry(
                section.as_str(),
                subsection.as_deref(),
                value.as_str(),
                path,
                workspace_root,
            )
        {
            if merge_git_identity_config_file(
                config,
                include_path.as_path(),
                workspace_root,
                visited_paths,
            ) {
                saw_any = true;
            }
            continue;
        }

        match (section.as_str(), subsection.as_deref(), key.as_str()) {
            ("user", None, "name") => {
                if !value.is_empty() {
                    config.name = Some(value);
                    saw_any = true;
                }
            }
            ("user", None, "email") => {
                if !value.is_empty() {
                    config.email = Some(value);
                    saw_any = true;
                }
            }
            _ => {}
        }
    }

    saw_any
}

fn parse_git_config_section_header(header: &str) -> (String, Option<String>) {
    let mut parts = header.splitn(2, char::is_whitespace);
    let section = parts.next().unwrap_or_default().trim().to_ascii_lowercase();
    let subsection = parts
        .next()
        .map(str::trim)
        .map(normalize_git_config_value)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    (section, subsection)
}

fn git_include_path_for_entry(
    section: &str,
    subsection: Option<&str>,
    include_path_raw: &str,
    config_path: &Path,
    workspace_root: &Path,
) -> Option<PathBuf> {
    if section == "include" {
        return resolve_git_include_path(include_path_raw, config_path);
    }

    if section != "includeif"
        || !matches_include_if_condition(subsection, config_path, workspace_root)
    {
        return None;
    }

    resolve_git_include_path(include_path_raw, config_path)
}

fn resolve_git_include_path(include_path_raw: &str, config_path: &Path) -> Option<PathBuf> {
    let include_path_raw = include_path_raw.trim();
    if include_path_raw.is_empty() {
        return None;
    }

    let include_path = if let Some(suffix) = include_path_raw.strip_prefix("~/") {
        dirs::home_dir().map(|home| home.join(suffix))?
    } else {
        let path = PathBuf::from(include_path_raw);
        if path.is_absolute() {
            path
        } else {
            config_path.parent()?.join(path)
        }
    };

    Some(include_path)
}

fn matches_include_if_condition(
    subsection: Option<&str>,
    config_path: &Path,
    workspace_root: &Path,
) -> bool {
    let Some(subsection) = subsection else {
        return false;
    };

    let Some(workspace_git_dir) = workspace_git_dir_path(workspace_root) else {
        return false;
    };

    if let Some(pattern) = subsection.strip_prefix("gitdir/i:") {
        return gitdir_pattern_matches(pattern, config_path, workspace_git_dir.as_path(), true);
    }
    if let Some(pattern) = subsection.strip_prefix("gitdir:") {
        return gitdir_pattern_matches(pattern, config_path, workspace_git_dir.as_path(), false);
    }

    false
}

fn gitdir_pattern_matches(
    raw_pattern: &str,
    config_path: &Path,
    workspace_git_dir: &Path,
    case_insensitive: bool,
) -> bool {
    let Some(resolved_pattern) =
        resolve_gitdir_condition_pattern(raw_pattern, config_path, case_insensitive)
    else {
        return false;
    };
    let workspace = normalize_gitdir_match_text(workspace_git_dir, case_insensitive);
    wildcard_pattern_matches(resolved_pattern.as_str(), workspace.as_str())
}

fn resolve_gitdir_condition_pattern(
    raw_pattern: &str,
    config_path: &Path,
    case_insensitive: bool,
) -> Option<String> {
    let pattern = raw_pattern.trim();
    if pattern.is_empty() {
        return None;
    }

    let resolved = if let Some(suffix) = pattern.strip_prefix("~/") {
        dirs::home_dir().map(|home| home.join(suffix))?
    } else {
        let path = PathBuf::from(pattern);
        if path.is_absolute() {
            path
        } else {
            config_path.parent()?.join(path)
        }
    };
    Some(normalize_gitdir_match_text(
        resolved.as_path(),
        case_insensitive,
    ))
}

fn normalize_gitdir_match_text(path: &Path, case_insensitive: bool) -> String {
    let mut text = path.to_string_lossy().replace('\\', "/");
    if case_insensitive {
        text = text.to_ascii_lowercase();
    }
    text
}

fn wildcard_pattern_matches(pattern: &str, target: &str) -> bool {
    if !pattern.contains('*') {
        if pattern.ends_with('/') {
            let trimmed = pattern.trim_end_matches('/');
            return target == trimmed || target.starts_with(pattern);
        }
        return target.starts_with(pattern);
    }

    let mut remainder = target;
    for (ix, segment) in pattern.split('*').enumerate() {
        if segment.is_empty() {
            continue;
        }
        if ix == 0 && !pattern.starts_with('*') {
            if !remainder.starts_with(segment) {
                return false;
            }
            remainder = &remainder[segment.len()..];
            continue;
        }

        let Some(offset) = remainder.find(segment) else {
            return false;
        };
        remainder = &remainder[offset + segment.len()..];
    }

    if pattern.ends_with('*') {
        true
    } else {
        remainder.is_empty()
    }
}

fn normalize_git_config_value(value: &str) -> String {
    let value = value.trim();
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        value[1..value.len() - 1].trim().to_string()
    } else {
        value.to_string()
    }
}

fn parse_git_config_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

fn load_config_if_exists(
    config: &mut StackedConfig,
    source: ConfigSource,
    path: PathBuf,
) -> Result<()> {
    if path.is_file() {
        config
            .load_file(source, path.clone())
            .with_context(|| format!("failed to load jj config {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("hunk-{prefix}-{unique}"));
            fs::create_dir_all(&path).expect("temp directory should be created");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory should be created");
        }
        fs::write(path, contents).expect("file should be written");
    }

    #[test]
    fn identity_config_resolves_include_and_matching_include_if_chain() {
        let fixture = TempDir::new("git-identity-include-chain");
        let workspace_root = fixture.path.join("repo");
        fs::create_dir_all(&workspace_root).expect("workspace root should be created");
        fs::create_dir_all(workspace_root.join(".git")).expect("workspace .git should be created");
        let root_config = fixture.path.join("root.gitconfig");
        let child_config = fixture.path.join("child.gitconfig");
        let scoped_config = fixture.path.join("scoped.gitconfig");

        write_file(
            &root_config,
            "\
[include]
    path = child.gitconfig
[user]
    name = Main Name
[includeIf \"gitdir:repo/\"]
    path = scoped.gitconfig
",
        );
        write_file(
            &child_config,
            "\
[user]
    name = Child Name
    email = child@example.com
",
        );
        write_file(
            &scoped_config,
            "\
[user]
    email = scoped@example.com
",
        );

        let mut config = GitIdentityConfig::default();
        let mut visited = BTreeSet::new();
        let saw_any = merge_git_identity_config_file(
            &mut config,
            root_config.as_path(),
            workspace_root.as_path(),
            &mut visited,
        );

        assert!(saw_any);
        assert_eq!(config.name.as_deref(), Some("Main Name"));
        assert_eq!(config.email.as_deref(), Some("scoped@example.com"));
    }

    #[test]
    fn signing_config_skips_non_matching_include_if() {
        let fixture = TempDir::new("git-signing-includeif-skip");
        let workspace_root = fixture.path.join("repo");
        fs::create_dir_all(&workspace_root).expect("workspace root should be created");
        fs::create_dir_all(workspace_root.join(".git")).expect("workspace .git should be created");
        let root_config = fixture.path.join("root.gitconfig");
        let child_config = fixture.path.join("child.gitconfig");
        let skipped_config = fixture.path.join("skipped.gitconfig");

        write_file(
            &root_config,
            "\
[include]
    path = child.gitconfig
[includeIf \"gitdir:other/\"]
    path = skipped.gitconfig
",
        );
        write_file(
            &child_config,
            "\
[commit]
    gpgSign = true
[gpg]
    format = ssh
",
        );
        write_file(
            &skipped_config,
            "\
[user]
    signingKey = should-not-load
",
        );

        let mut config = GitSigningConfig::default();
        let mut visited = BTreeSet::new();
        let saw_any = merge_git_signing_config_file(
            &mut config,
            root_config.as_path(),
            workspace_root.as_path(),
            &mut visited,
        );

        assert!(saw_any);
        assert_eq!(config.commit_gpgsign, Some(true));
        assert_eq!(config.gpg_format.as_deref(), Some("ssh"));
        assert!(config.signing_key.is_none());
    }

    #[test]
    fn include_if_gitdir_uses_git_target_when_workspace_has_no_dot_git_dir() {
        let fixture = TempDir::new("git-includeif-git-target");
        let workspace_root = fixture.path.join("workspace");
        fs::create_dir_all(workspace_root.join(".jj").join("repo").join("store"))
            .expect("workspace store path should be created");
        let external_git_dir = fixture.path.join("external").join(".git");
        fs::create_dir_all(&external_git_dir).expect("external git dir should be created");
        write_file(
            &workspace_root.join(".jj").join("repo").join("store").join("git_target"),
            external_git_dir.to_string_lossy().as_ref(),
        );

        let root_config = fixture.path.join("root.gitconfig");
        let scoped_config = fixture.path.join("scoped.gitconfig");
        write_file(
            &root_config,
            "\
[includeIf \"gitdir:external/.git/\"]
    path = scoped.gitconfig
",
        );
        write_file(
            &scoped_config,
            "\
[user]
    name = Target Name
",
        );

        let mut config = GitIdentityConfig::default();
        let mut visited = BTreeSet::new();
        let saw_any = merge_git_identity_config_file(
            &mut config,
            root_config.as_path(),
            workspace_root.as_path(),
            &mut visited,
        );

        assert!(saw_any);
        assert_eq!(config.name.as_deref(), Some("Target Name"));
    }
}
