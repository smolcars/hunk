#[derive(Clone, Copy)]
pub(crate) struct LoadedFileEditorReuseState<'a> {
    pub(crate) requested_path: &'a str,
    pub(crate) current_editor_path: Option<&'a str>,
    pub(crate) editor_loading: bool,
    pub(crate) editor_error: Option<&'a str>,
    pub(crate) has_document: bool,
}

pub(crate) fn should_reuse_loaded_file_editor(state: LoadedFileEditorReuseState<'_>) -> bool {
    state.current_editor_path == Some(state.requested_path)
        && state.editor_error.is_none()
        && (state.editor_loading || state.has_document)
}
