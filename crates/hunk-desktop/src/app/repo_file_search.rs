use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use super::fuzzy_match::{is_match_boundary, segment_prefix_position, subsequence_match_score};

#[derive(Debug, Clone)]
struct SearchableRepoFile {
    path: String,
    normalized_path: String,
    normalized_file_name: String,
}

#[derive(Debug, Default)]
struct RepoFileSearchState {
    repo_root: Option<PathBuf>,
    files: Arc<[SearchableRepoFile]>,
    reload_generation: u64,
}

#[derive(Debug, Clone)]
struct RankedRepoFile<'a> {
    file: &'a SearchableRepoFile,
    score: i32,
}

pub(crate) struct RepoFileSearchProvider {
    state: RwLock<RepoFileSearchState>,
}

impl RepoFileSearchProvider {
    pub(crate) fn new() -> Self {
        Self {
            state: RwLock::new(RepoFileSearchState::default()),
        }
    }

    pub(crate) fn begin_reload(&self, repo_root: Option<PathBuf>) -> u64 {
        let mut state = self.write_state();
        state.reload_generation = state.reload_generation.wrapping_add(1);
        if state.repo_root.as_ref() != repo_root.as_ref() {
            state.files = Arc::from(Vec::<SearchableRepoFile>::new());
        }
        state.repo_root = repo_root;
        state.reload_generation
    }

    pub(crate) fn apply_reload(
        &self,
        generation: u64,
        repo_root: &Path,
        paths: Vec<String>,
    ) -> bool {
        let mut state = self.write_state();
        if state.reload_generation != generation || state.repo_root.as_deref() != Some(repo_root) {
            return false;
        }

        state.files = searchable_repo_files(paths);
        true
    }

    pub(crate) fn clear(&self) {
        let _ = self.begin_reload(None);
    }

    pub(crate) fn matched_paths(&self, query: &str, limit: usize) -> Vec<String> {
        let files = self.read_state().files.clone();
        if files.is_empty() || limit == 0 {
            return Vec::new();
        }

        if query.trim().is_empty() {
            return files
                .iter()
                .take(limit)
                .map(|file| file.path.clone())
                .collect();
        }

        ranked_file_matches(files.as_ref(), query, limit)
            .into_iter()
            .map(|ranked| ranked.file.path.clone())
            .collect()
    }

    fn read_state(&self) -> RwLockReadGuard<'_, RepoFileSearchState> {
        match self.state.read() {
            Ok(guard) => guard,
            Err(error) => error.into_inner(),
        }
    }

    fn write_state(&self) -> RwLockWriteGuard<'_, RepoFileSearchState> {
        match self.state.write() {
            Ok(guard) => guard,
            Err(error) => error.into_inner(),
        }
    }
}

impl Default for RepoFileSearchProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn searchable_repo_files(paths: Vec<String>) -> Arc<[SearchableRepoFile]> {
    paths
        .into_iter()
        .map(|path| SearchableRepoFile {
            normalized_path: normalize_file_match_key(path.as_str()),
            normalized_file_name: normalize_file_match_key(file_name_from_path(path.as_str())),
            path,
        })
        .collect::<Vec<_>>()
        .into()
}

fn ranked_file_matches<'a>(
    files: &'a [SearchableRepoFile],
    query: &str,
    limit: usize,
) -> Vec<RankedRepoFile<'a>> {
    let mut ranked = Vec::with_capacity(limit);

    for file in files {
        let Some(score) = file_match_score(query, file) else {
            continue;
        };

        ranked.push(RankedRepoFile { file, score });
        ranked.sort_by(compare_ranked_repo_files);
        if ranked.len() > limit {
            ranked.truncate(limit);
        }
    }

    ranked
}

fn compare_ranked_repo_files(left: &RankedRepoFile<'_>, right: &RankedRepoFile<'_>) -> Ordering {
    right
        .score
        .cmp(&left.score)
        .then_with(|| left.file.path.len().cmp(&right.file.path.len()))
        .then_with(|| left.file.path.cmp(&right.file.path))
}

fn file_match_score(query: &str, file: &SearchableRepoFile) -> Option<i32> {
    let query = normalize_file_match_key(query);
    if query.is_empty() {
        return Some(0);
    }

    let candidate = file.normalized_path.as_str();
    let file_name = file.normalized_file_name.as_str();
    if candidate.is_empty() {
        return None;
    }

    let mut best_score = None;

    if candidate == query {
        best_score = Some(10_000);
    }

    if file_name == query {
        best_score = Some(best_score.map_or(9_600, |current| current.max(9_600)));
    }

    if file_name.starts_with(query.as_str()) {
        let score = 8_900 - (file_name.len() as i32 - query.len() as i32).max(0);
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    if let Some(position) = file_name.find(query.as_str()) {
        let boundary_bonus = if position == 0
            || is_match_boundary(file_name.as_bytes()[position.saturating_sub(1)])
        {
            220
        } else {
            0
        };
        let score = 8_000 + boundary_bonus
            - (position as i32 * 12)
            - (file_name.len() as i32 - query.len() as i32).max(0);
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    if candidate.starts_with(query.as_str()) {
        let score = 8_400 - (candidate.len() as i32 - query.len() as i32).max(0);
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    if let Some(position) = segment_prefix_position(candidate, query.as_str()) {
        let score =
            7_200 - (position as i32 * 8) - (candidate.len() as i32 - query.len() as i32).max(0);
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    if let Some(position) = candidate.find(query.as_str()) {
        let boundary_bonus = if position == 0
            || is_match_boundary(candidate.as_bytes()[position.saturating_sub(1)])
        {
            180
        } else {
            0
        };
        let score = 6_400 + boundary_bonus
            - (position as i32 * 10)
            - (candidate.len() as i32 - query.len() as i32).max(0);
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    if let Some(score) = subsequence_match_score(candidate, query.as_str()) {
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    best_score
}

fn file_name_from_path(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn normalize_file_match_key(value: &str) -> String {
    value.trim().to_lowercase().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        RepoFileSearchProvider, SearchableRepoFile, file_match_score, normalize_file_match_key,
    };

    fn searchable_file(path: &str) -> SearchableRepoFile {
        SearchableRepoFile {
            path: path.to_string(),
            normalized_path: normalize_file_match_key(path),
            normalized_file_name: normalize_file_match_key(path.rsplit('/').next().unwrap_or(path)),
        }
    }

    #[test]
    fn file_match_score_prefers_exact_filename_matches() {
        let main = searchable_file("src/main.rs");
        let mod_rs = searchable_file("src/mod.rs");
        let nested = searchable_file("tests/main.rs");

        let main_score = file_match_score("main.rs", &main).expect("main.rs should match");
        let nested_score =
            file_match_score("main.rs", &nested).expect("tests/main.rs should match");
        let mod_score = file_match_score("main.rs", &mod_rs).unwrap_or(i32::MIN);

        assert!(main_score >= nested_score);
        assert!(nested_score > mod_score);
    }

    #[test]
    fn matched_paths_returns_prefix_results_for_empty_query() {
        let provider = RepoFileSearchProvider::new();
        let generation = provider.begin_reload(Some(PathBuf::from("/repo")));
        assert!(provider.apply_reload(
            generation,
            Path::new("/repo"),
            vec![
                "src/main.rs".into(),
                "README.md".into(),
                "tests/app.rs".into()
            ],
        ));

        assert_eq!(
            provider.matched_paths("", 2),
            vec!["src/main.rs".to_string(), "README.md".to_string()]
        );
    }
}
