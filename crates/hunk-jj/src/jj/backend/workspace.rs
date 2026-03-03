pub(super) fn walk_repo_tree(
    root: &Path,
    current: &Path,
    tracked_paths: &BTreeSet<String>,
    entries: &mut Vec<RepoTreeEntry>,
) -> Result<()> {
    if entries.len() >= MAX_REPO_TREE_ENTRIES {
        return Ok(());
    }

    let mut children = read_dir_sorted(current)?;
    for child in children.drain(..) {
        if entries.len() >= MAX_REPO_TREE_ENTRIES {
            break;
        }

        let name = child.file_name();
        let name = name.to_string_lossy();
        if name == ".git" || name == ".jj" {
            continue;
        }

        let Ok(file_type) = child.file_type() else {
            continue;
        };

        let child_path = child.path();
        let Ok(relative) = child_path.strip_prefix(root) else {
            continue;
        };
        let relative_path = normalize_path(&relative.to_string_lossy());
        if relative_path.is_empty() {
            continue;
        }

        if file_type.is_dir() {
            let ignored = !path_is_tracked_or_ancestor(relative_path.as_str(), tracked_paths);
            entries.push(RepoTreeEntry {
                path: relative_path,
                kind: RepoTreeEntryKind::Directory,
                ignored,
            });
            if ignored {
                continue;
            }
            walk_repo_tree(root, &child_path, tracked_paths, entries)?;
            continue;
        }

        if file_type.is_file() {
            let ignored = !tracked_paths.contains(relative_path.as_str());
            entries.push(RepoTreeEntry {
                path: relative_path,
                kind: RepoTreeEntryKind::File,
                ignored,
            });
        }
    }

    Ok(())
}

fn read_dir_sorted(path: &Path) -> Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(path)
        .with_context(|| format!("failed to read directory {}", path.display()))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.file_name()
            .to_string_lossy()
            .cmp(&right.file_name().to_string_lossy())
    });
    Ok(entries)
}

fn path_is_tracked_or_ancestor(path: &str, tracked_paths: &BTreeSet<String>) -> bool {
    if tracked_paths.contains(path) {
        return true;
    }

    let prefix = format!("{path}/");
    tracked_paths
        .iter()
        .any(|tracked| tracked.starts_with(&prefix))
}

pub(super) fn normalize_path(path: &str) -> String {
    path.trim().trim_end_matches('/').to_string()
}

pub(super) fn nested_repo_roots_from_fs(root: &Path) -> Result<BTreeSet<String>> {
    let mut nested_roots = BTreeSet::new();
    collect_nested_repo_roots(root, root, &mut nested_roots)?;
    Ok(nested_roots)
}

fn collect_nested_repo_roots(
    root: &Path,
    current: &Path,
    nested_roots: &mut BTreeSet<String>,
) -> Result<()> {
    for child in read_dir_sorted(current)? {
        let Ok(file_type) = child.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        let name = child.file_name();
        let name = name.to_string_lossy();
        if name == ".git" || name == ".jj" {
            continue;
        }

        let child_path = child.path();
        if directory_is_repo_root(child_path.as_path()) {
            if let Ok(relative) = child_path.strip_prefix(root) {
                let relative_path = normalize_path(&relative.to_string_lossy());
                if !relative_path.is_empty() {
                    nested_roots.insert(relative_path);
                }
            }
            continue;
        }

        collect_nested_repo_roots(root, child_path.as_path(), nested_roots)?;
    }

    Ok(())
}

fn directory_is_repo_root(path: &Path) -> bool {
    let git_marker = path.join(".git");
    let jj_marker = path.join(".jj");
    git_marker.is_dir() || git_marker.is_file() || jj_marker.is_dir()
}

pub(super) fn discover_repo_root(cwd: &Path) -> Result<PathBuf> {
    if let Some(root) = find_jj_repo_ancestor(cwd) {
        return Ok(root);
    }

    if let Some(git_root) = find_git_repo_ancestor(cwd) {
        initialize_jj_for_git_repo(&git_root)
            .context("failed to auto-initialize JJ repository in Git checkout")?;

        if let Some(root) = find_jj_repo_ancestor(cwd).or_else(|| find_jj_repo_ancestor(&git_root))
        {
            return Ok(root);
        }
    }

    Err(anyhow!("There is no jj repo in '{}'", cwd.display()))
        .context("failed to discover jj repository")
}

fn initialize_jj_for_git_repo(git_root: &Path) -> Result<()> {
    if git_root.join(".jj").is_dir() {
        return Ok(());
    }

    let settings = load_user_settings(Some(git_root))?;
    let git_repo_path = git_root.join(".git");
    Workspace::init_external_git(&settings, git_root, &git_repo_path).with_context(|| {
        format!(
            "failed to initialize colocated JJ repo at {}",
            git_root.display()
        )
    })?;

    let jj_root = git_root.join(".jj");

    let jj_ignore = jj_root.join(".gitignore");
    if !jj_ignore.is_file() {
        fs::write(&jj_ignore, "/*\n")
            .with_context(|| format!("failed to write {}", jj_ignore.display()))?;
    }

    Ok(())
}

fn find_jj_repo_ancestor(path: &Path) -> Option<PathBuf> {
    let mut current = if path.is_file() {
        path.parent()
    } else {
        Some(path)
    };

    while let Some(dir) = current {
        if dir.join(".jj").is_dir() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }

    None
}

fn find_git_repo_ancestor(path: &Path) -> Option<PathBuf> {
    let mut current = if path.is_file() {
        path.parent()
    } else {
        Some(path)
    };

    while let Some(dir) = current {
        let marker = dir.join(".git");
        if marker.is_dir() || marker.is_file() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }

    None
}
