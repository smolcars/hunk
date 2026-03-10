fn repo_tree_contains_path(nodes: &[RepoTreeNode], path: &str) -> bool {
    for node in nodes {
        if node.path == path {
            return true;
        }
        if repo_tree_contains_path(&node.children, path) {
            return true;
        }
    }
    false
}

fn repo_tree_has_directory(nodes: &[RepoTreeNode], path: &str) -> bool {
    for node in nodes {
        if node.kind == RepoTreeNodeKind::Directory && node.path == path {
            return true;
        }
        if repo_tree_has_directory(&node.children, path) {
            return true;
        }
    }
    false
}

fn repo_tree_base_dir(path: &str, kind: RepoTreeNodeKind) -> anyhow::Result<Option<String>> {
    match kind {
        RepoTreeNodeKind::Directory => Ok(Some(normalize_repo_relative_path_str(path)?)),
        RepoTreeNodeKind::File => Ok(repo_relative_parent_dir(path)),
    }
}

fn normalize_repo_relative_path(path: &str) -> anyhow::Result<PathBuf> {
    let raw = path.trim();
    if raw.is_empty() {
        anyhow::bail!("Path cannot be empty.");
    }
    let candidate = Path::new(raw);
    if candidate.is_absolute() {
        anyhow::bail!("Path must be relative to the repository root.");
    }

    let mut normalized = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => anyhow::bail!("Path cannot contain `..`."),
            Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("Path must be relative to the repository root.")
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        anyhow::bail!("Path cannot be empty.");
    }
    Ok(normalized)
}

fn normalize_repo_relative_path_str(path: &str) -> anyhow::Result<String> {
    Ok(repo_relative_path_from_pathbuf(&normalize_repo_relative_path(path)?))
}

fn repo_relative_path_from_pathbuf(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn repo_relative_parent_dir(path: &str) -> Option<String> {
    let normalized = normalize_repo_relative_path(path).ok()?;
    let parent = normalized.parent()?;
    if parent.as_os_str().is_empty() {
        return None;
    }
    Some(repo_relative_path_from_pathbuf(parent))
}

fn file_name_from_repo_path(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
}

fn join_repo_relative(base_dir: Option<&str>, requested_path: &str) -> anyhow::Result<String> {
    let requested = normalize_repo_relative_path(requested_path)?;
    let joined = if let Some(base_dir) = base_dir {
        let mut base = normalize_repo_relative_path(base_dir)?;
        base.push(requested);
        base
    } else {
        requested
    };
    Ok(repo_relative_path_from_pathbuf(&joined))
}

fn rename_destination_path(source_path: &str, requested_name: &str) -> anyhow::Result<String> {
    let trimmed = requested_name.trim();
    if trimmed.is_empty() {
        anyhow::bail!("New file name cannot be empty.");
    }
    let candidate = Path::new(trimmed);
    if candidate.components().count() != 1 {
        anyhow::bail!("Rename expects a file name, not a path.");
    }
    let file_name = candidate
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid file name."))?;

    let source = normalize_repo_relative_path(source_path)?;
    let Some(parent) = source.parent() else {
        anyhow::bail!("Cannot resolve parent directory for `{source_path}`.");
    };
    let destination = if parent.as_os_str().is_empty() {
        PathBuf::from(file_name)
    } else {
        parent.join(file_name)
    };
    Ok(repo_relative_path_from_pathbuf(&destination))
}

fn fs_create_repo_tree_file(repo_root: &Path, relative_path: &str) -> anyhow::Result<()> {
    let normalized = normalize_repo_relative_path(relative_path)?;
    let absolute = repo_root.join(&normalized);
    if absolute.exists() {
        anyhow::bail!("`{}` already exists.", normalized.display());
    }
    let parent = absolute.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "Cannot create `{}` because parent directory is unavailable.",
            normalized.display()
        )
    })?;
    if !parent.exists() {
        anyhow::bail!(
            "Parent directory does not exist for `{}`.",
            normalized.display()
        );
    }

    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&absolute)
        .with_context(|| format!("failed to create {}", absolute.display()))?;
    Ok(())
}

fn fs_create_repo_tree_directory(repo_root: &Path, relative_path: &str) -> anyhow::Result<()> {
    let normalized = normalize_repo_relative_path(relative_path)?;
    let absolute = repo_root.join(&normalized);
    if absolute.exists() {
        anyhow::bail!("`{}` already exists.", normalized.display());
    }
    std::fs::create_dir_all(&absolute)
        .with_context(|| format!("failed to create {}", absolute.display()))?;
    Ok(())
}

fn fs_rename_repo_tree_file(
    repo_root: &Path,
    source_path: &str,
    destination_path: &str,
) -> anyhow::Result<()> {
    let source = normalize_repo_relative_path(source_path)?;
    let destination = normalize_repo_relative_path(destination_path)?;
    let source_absolute = repo_root.join(&source);
    let destination_absolute = repo_root.join(&destination);

    if !source_absolute.is_file() {
        anyhow::bail!("`{}` is not a file.", source.display());
    }
    if destination_absolute.exists() {
        anyhow::bail!("`{}` already exists.", destination.display());
    }

    let destination_parent = destination_absolute.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "Cannot rename `{}` because destination parent is unavailable.",
            source.display()
        )
    })?;
    if !destination_parent.exists() {
        anyhow::bail!(
            "Destination parent directory does not exist for `{}`.",
            destination.display()
        );
    }

    std::fs::rename(&source_absolute, &destination_absolute).with_context(|| {
        format!(
            "failed to rename {} to {}",
            source_absolute.display(),
            destination_absolute.display()
        )
    })?;
    Ok(())
}

fn fs_delete_repo_tree_file(repo_root: &Path, path: &str) -> anyhow::Result<()> {
    let normalized = normalize_repo_relative_path(path)?;
    let absolute = repo_root.join(&normalized);
    if !absolute.is_file() {
        anyhow::bail!("`{}` is not a file.", normalized.display());
    }
    std::fs::remove_file(&absolute)
        .with_context(|| format!("failed to delete {}", absolute.display()))?;
    Ok(())
}
