use crate::app::AiPendingSteer;

fn accepted_after_sequence_for_pending_steer(
    state: &AiState,
    thread_id: &str,
    turn_id: &str,
) -> u64 {
    state
        .turns
        .get(hunk_codex::state::turn_storage_key(thread_id, turn_id).as_str())
        .map(|turn| turn.last_sequence)
        .or_else(|| {
            state
                .threads
                .get(thread_id)
                .map(|thread| thread.last_sequence)
        })
        .unwrap_or(0)
}

fn pending_steer_from_send_prompt(
    thread_id: String,
    turn_id: String,
    prompt: Option<&str>,
    local_image_paths: &[PathBuf],
    accepted_after_sequence: u64,
) -> AiPendingSteer {
    AiPendingSteer {
        thread_id,
        turn_id,
        prompt: prompt.unwrap_or_default().to_string(),
        local_images: local_image_paths.to_vec(),
        accepted_after_sequence,
        started_at: Instant::now(),
    }
}
