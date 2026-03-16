#[path = "../src/app/markdown_links.rs"]
mod markdown_links;

use std::fs;

use hunk_domain::markdown_preview::{MarkdownInlineSpan, MarkdownInlineStyle};
use markdown_links::{
    MarkdownLinkTarget, MarkdownWorkspaceFileLink, markdown_inline_text_and_link_ranges,
    resolve_markdown_link_target,
};

#[test]
fn markdown_link_ranges_merge_adjacent_link_spans() {
    let link_style = MarkdownInlineStyle {
        link: Some("https://example.com".to_string()),
        ..MarkdownInlineStyle::default()
    };
    let spans = vec![
        MarkdownInlineSpan {
            text: "See ".to_string(),
            style: MarkdownInlineStyle::default(),
        },
        MarkdownInlineSpan {
            text: "docs".to_string(),
            style: link_style.clone(),
        },
        MarkdownInlineSpan {
            text: " now".to_string(),
            style: link_style,
        },
    ];

    let (text, link_ranges) = markdown_inline_text_and_link_ranges(&spans);

    assert_eq!(text, "See docs now");
    assert_eq!(link_ranges.len(), 1);
    assert_eq!(link_ranges[0].range, 4..12);
    assert_eq!(link_ranges[0].raw_target, "https://example.com");
}

#[test]
fn resolves_workspace_file_links_inside_root() {
    let root = test_temp_dir("resolve-workspace-file-links");
    let nested = root.join("src");
    fs::create_dir_all(&nested).expect("create nested workspace dir");
    let file_path = nested.join("main.rs");
    fs::write(&file_path, "fn main() {}\n").expect("write workspace file");

    let relative = resolve_markdown_link_target("src/main.rs#L72", Some(root.as_path()));
    assert_eq!(
        relative,
        Some(MarkdownLinkTarget::WorkspaceFile(
            MarkdownWorkspaceFileLink {
                raw_target: "src/main.rs#L72".to_string(),
                normalized_path: "src/main.rs".to_string(),
                line: Some(72),
            }
        ))
    );

    let absolute_target = format!("{}:9", file_path.display());
    let absolute = resolve_markdown_link_target(absolute_target.as_str(), Some(root.as_path()));
    assert_eq!(
        absolute,
        Some(MarkdownLinkTarget::WorkspaceFile(
            MarkdownWorkspaceFileLink {
                raw_target: absolute_target,
                normalized_path: "src/main.rs".to_string(),
                line: Some(9),
            }
        ))
    );
}

#[test]
fn rejects_workspace_file_links_outside_root() {
    let root = test_temp_dir("reject-workspace-file-links");
    fs::create_dir_all(root.as_path()).expect("create workspace root");

    let outside_root = test_temp_dir("outside-workspace-file-links");
    let outside_file = outside_root.join("secrets.txt");
    fs::write(&outside_file, "secret\n").expect("write outside file");

    assert_eq!(
        resolve_markdown_link_target(
            outside_file.to_string_lossy().as_ref(),
            Some(root.as_path())
        ),
        None
    );
}

fn test_temp_dir(label: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("hunk-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(path.as_path());
    path
}
