use std::path::Path;
use std::time::Duration;

use hunk_codex::protocol::ReasoningEffort;
use hunk_codex::structured_generation::{StructuredGenerationRequest, generate_structured_output};

use crate::app::ai_terminal_safety::{
    TerminalAutoReviewAssessment, TerminalAutoReviewParseError, TerminalAutoReviewRequest,
    parse_terminal_auto_review_assessment, terminal_auto_review_output_schema,
    terminal_auto_review_prompt,
};

const TERMINAL_AUTO_REVIEW_MODEL: &str = "gpt-5.4-mini";
const TERMINAL_AUTO_REVIEW_TIMEOUT: Duration = Duration::from_secs(8);

pub(crate) fn run_terminal_auto_review(
    codex_home: &Path,
    cwd: &Path,
    codex_executable: &Path,
    request: &TerminalAutoReviewRequest,
    fallback_model: Option<&str>,
) -> Result<TerminalAutoReviewAssessment, TerminalAutoReviewParseError> {
    let primary = run_terminal_auto_review_with_model(
        codex_home,
        cwd,
        codex_executable,
        request,
        TERMINAL_AUTO_REVIEW_MODEL,
    );
    let fallback_model = fallback_model
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .filter(|model| *model != TERMINAL_AUTO_REVIEW_MODEL);
    match (primary, fallback_model) {
        (Ok(assessment), _) => Ok(assessment),
        (Err(primary_error), Some(fallback_model))
            if !terminal_auto_review_error_is_timeout(&primary_error) =>
        {
            run_terminal_auto_review_with_model(
                codex_home,
                cwd,
                codex_executable,
                request,
                fallback_model,
            )
            .map_err(|fallback_error| TerminalAutoReviewParseError {
                message: format!(
                    "{}; fallback model {fallback_model} also failed: {}",
                    primary_error.message, fallback_error.message
                ),
            })
        }
        (Err(primary_error), None) => Err(primary_error),
        (Err(primary_error), Some(_)) => Err(primary_error),
    }
}

fn run_terminal_auto_review_with_model(
    codex_home: &Path,
    cwd: &Path,
    codex_executable: &Path,
    request: &TerminalAutoReviewRequest,
    model: &str,
) -> Result<TerminalAutoReviewAssessment, TerminalAutoReviewParseError> {
    let prompt = terminal_auto_review_prompt(request);
    let output_schema = terminal_auto_review_output_schema();
    let output = generate_structured_output(StructuredGenerationRequest {
        codex_home,
        cwd,
        codex_executable,
        prompt: prompt.as_str(),
        output_schema: &output_schema,
        image_paths: &[],
        model: Some(model),
        reasoning_effort: ReasoningEffort::Low,
        client_name: "hunk-desktop-terminal-review",
        client_version: env!("CARGO_PKG_VERSION"),
        timeout: TERMINAL_AUTO_REVIEW_TIMEOUT,
    })
    .map_err(|error| TerminalAutoReviewParseError {
        message: format!("terminal auto-review session failed: {error}"),
    })?;
    parse_terminal_auto_review_assessment(output.to_string().as_str())
}

fn terminal_auto_review_error_is_timeout(error: &TerminalAutoReviewParseError) -> bool {
    let message = error.message.to_ascii_lowercase();
    message.contains("timeout") || message.contains("timed out")
}
