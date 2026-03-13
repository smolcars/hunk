pub(crate) fn segment_prefix_position(candidate: &str, query: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }

    let mut offset = 0usize;
    for segment in candidate.split('/') {
        if segment.starts_with(query) {
            return Some(offset);
        }
        offset = offset.saturating_add(segment.len() + 1);
    }

    None
}

pub(crate) fn subsequence_match_score(candidate: &str, query: &str) -> Option<i32> {
    let candidate = candidate.as_bytes();
    let query = query.as_bytes();

    if query.is_empty() {
        return Some(0);
    }

    let mut query_ix = 0usize;
    let mut score = 2_000i32;
    let mut last_match_ix = None::<usize>;

    for (candidate_ix, candidate_byte) in candidate.iter().copied().enumerate() {
        if candidate_byte != query[query_ix] {
            continue;
        }

        score += 18;

        if candidate_ix == 0 || is_match_boundary(candidate[candidate_ix.saturating_sub(1)]) {
            score += 30;
        }

        match last_match_ix {
            Some(previous_ix) if candidate_ix == previous_ix + 1 => {
                score += 24;
            }
            Some(previous_ix) => {
                score -= (candidate_ix.saturating_sub(previous_ix + 1) as i32).min(18);
            }
            None => {
                score -= candidate_ix as i32;
            }
        }

        last_match_ix = Some(candidate_ix);
        query_ix += 1;

        if query_ix == query.len() {
            score -= (candidate.len() as i32 - query.len() as i32).max(0);
            return Some(score);
        }
    }

    None
}

pub(crate) fn is_match_boundary(byte: u8) -> bool {
    matches!(byte, b'/' | b'-' | b'_' | b'.')
}
