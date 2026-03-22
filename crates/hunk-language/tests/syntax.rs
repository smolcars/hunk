use std::path::Path;

use hunk_language::{
    CompletionTriggerKind, LanguageRegistry, ParseStatus, SemanticToken, SemanticTokenKind,
    SyntaxSession, merge_highlight_layers, semantic_token_captures,
};
use hunk_text::{TextPosition, TextRange};

#[test]
fn rust_source_parses_and_highlights_keywords() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let source = "fn main() {\n    let answer = 42;\n}\n";

    let snapshot = session
        .parse_for_path(&registry, Path::new("main.rs"), source)
        .expect("parse");
    assert_eq!(snapshot.parse_status, ParseStatus::Ready);

    let captures = session
        .highlight_visible_range(&registry, source, 0..source.len())
        .expect("highlights");
    assert!(
        captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );
    assert!(
        captures
            .iter()
            .any(|capture| { capture.style_key == "function" || capture.style_key == "variable" })
    );
}

#[test]
fn html_injection_highlights_embedded_javascript_and_css() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let source = "<html><body><script>const answer = 42;</script><style>.card { color: red; }</style></body></html>";

    session
        .parse_for_path(&registry, Path::new("index.html"), source)
        .expect("parse html");
    let captures = session
        .highlight_visible_range(&registry, source, 0..source.len())
        .expect("html highlights");

    let const_offset = source.find("const").expect("const");
    let color_offset = source.find("color").expect("color");
    assert!(captures.iter().any(|capture| {
        capture.style_key == "keyword"
            && capture.byte_range.start <= const_offset
            && capture.byte_range.end >= const_offset + "const".len()
    }));
    assert!(captures.iter().any(|capture| {
        capture.style_key == "property"
            && capture.byte_range.start <= color_offset
            && capture.byte_range.end >= color_offset + "color".len()
    }));
}

#[test]
fn typescript_source_uses_javascript_base_highlights() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let source = "import { parseBIP321 } from \"./index\";\nconst TEST_DATA = parseBIP321(\"bitcoin:addr\");\n";

    session
        .parse_for_path(&registry, Path::new("fixture.ts"), source)
        .expect("parse typescript");
    let captures = session
        .highlight_visible_range(&registry, source, 0..source.len())
        .expect("typescript highlights");

    let import_offset = source.find("import").expect("import");
    let const_offset = source.find("const").expect("const");
    let function_offset = source.find("parseBIP321").expect("function");

    assert!(captures.iter().any(|capture| {
        capture.style_key == "keyword"
            && capture.byte_range.start <= import_offset
            && capture.byte_range.end >= import_offset + "import".len()
    }));
    assert!(captures.iter().any(|capture| {
        capture.style_key == "keyword"
            && capture.byte_range.start <= const_offset
            && capture.byte_range.end >= const_offset + "const".len()
    }));
    assert!(captures.iter().any(|capture| {
        (capture.style_key == "function" || capture.style_key == "variable")
            && capture.byte_range.start <= function_offset
            && capture.byte_range.end >= function_offset + "parseBIP321".len()
    }));
}

#[test]
fn python_and_bash_sources_parse_and_highlight_keywords() {
    let registry = LanguageRegistry::builtin();

    let mut python = SyntaxSession::new();
    let python_source = "def main():\n    return 42\n";
    python
        .parse_for_path(&registry, Path::new("main.py"), python_source)
        .expect("parse python");
    let python_captures = python
        .highlight_visible_range(&registry, python_source, 0..python_source.len())
        .expect("python highlights");
    assert!(
        python_captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );

    let mut bash = SyntaxSession::new();
    let bash_source = "if [ -n \"$HOME\" ]; then\necho ok\nfi\n";
    bash.parse_for_path(&registry, Path::new("build.sh"), bash_source)
        .expect("parse bash");
    let bash_captures = bash
        .highlight_visible_range(&registry, bash_source, 0..bash_source.len())
        .expect("bash highlights");
    assert!(
        bash_captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );
}

#[test]
fn powershell_source_parses_and_highlights_keywords() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let source = "function Invoke-Build { Write-Host \"hi\" }\n";

    session
        .parse_for_path(&registry, Path::new("build.ps1"), source)
        .expect("parse powershell");
    let captures = session
        .highlight_visible_range(&registry, source, 0..source.len())
        .expect("powershell highlights");

    assert!(
        captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );
    assert!(
        captures
            .iter()
            .any(|capture| capture.style_key == "function" || capture.style_key == "property")
    );
}

#[test]
fn phase_one_languages_parse_and_highlight_representative_tokens() {
    let registry = LanguageRegistry::builtin();

    let mut java = SyntaxSession::new();
    let java_source = "class Main { static void main(String[] args) { return; } }\n";
    java.parse_for_path(&registry, Path::new("Main.java"), java_source)
        .expect("parse java");
    let java_captures = java
        .highlight_visible_range(&registry, java_source, 0..java_source.len())
        .expect("java highlights");
    assert!(
        java_captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );
    assert!(
        java_captures
            .iter()
            .any(|capture| capture.style_key == "type")
    );

    let mut c = SyntaxSession::new();
    let c_source = "int main(void) { return 0; }\n";
    c.parse_for_path(&registry, Path::new("main.c"), c_source)
        .expect("parse c");
    let c_captures = c
        .highlight_visible_range(&registry, c_source, 0..c_source.len())
        .expect("c highlights");
    assert!(
        c_captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );
    assert!(c_captures.iter().any(|capture| capture.style_key == "type"));

    let mut cpp = SyntaxSession::new();
    let cpp_source = "class Widget { public: int value() const { return 1; } };\n";
    cpp.parse_for_path(&registry, Path::new("main.cpp"), cpp_source)
        .expect("parse cpp");
    let cpp_captures = cpp
        .highlight_visible_range(&registry, cpp_source, 0..cpp_source.len())
        .expect("cpp highlights");
    assert!(
        cpp_captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );
    assert!(
        cpp_captures
            .iter()
            .any(|capture| capture.style_key == "function")
    );

    let mut csharp = SyntaxSession::new();
    let csharp_source = "class Program { static void Main() { Console.WriteLine(\"hi\"); } }\n";
    csharp
        .parse_for_path(&registry, Path::new("Program.cs"), csharp_source)
        .expect("parse csharp");
    let csharp_captures = csharp
        .highlight_visible_range(&registry, csharp_source, 0..csharp_source.len())
        .expect("csharp highlights");
    assert!(
        csharp_captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );
    assert!(
        csharp_captures
            .iter()
            .any(|capture| capture.style_key == "function")
    );

    let mut terraform = SyntaxSession::new();
    let terraform_source = "resource \"aws_s3_bucket\" \"logs\" { bucket = \"demo\" }\n";
    terraform
        .parse_for_path(&registry, Path::new("main.tf"), terraform_source)
        .expect("parse terraform");
    let terraform_captures = terraform
        .highlight_visible_range(&registry, terraform_source, 0..terraform_source.len())
        .expect("terraform highlights");
    assert!(
        terraform_captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );
    assert!(
        terraform_captures
            .iter()
            .any(|capture| capture.style_key == "property")
    );

    let mut swift = SyntaxSession::new();
    let swift_source = "class App { func run() { print(\"hi\") } }\n";
    swift
        .parse_for_path(&registry, Path::new("main.swift"), swift_source)
        .expect("parse swift");
    let swift_captures = swift
        .highlight_visible_range(&registry, swift_source, 0..swift_source.len())
        .expect("swift highlights");
    assert!(
        swift_captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );
    assert!(
        swift_captures
            .iter()
            .any(|capture| capture.style_key == "function")
    );
}

#[test]
fn phase_two_languages_parse_and_highlight_representative_tokens() {
    let registry = LanguageRegistry::builtin();

    let mut kotlin = SyntaxSession::new();
    let kotlin_source = "class App { fun run() { println(\"hi\") } }\n";
    kotlin
        .parse_for_path(&registry, Path::new("Main.kt"), kotlin_source)
        .expect("parse kotlin");
    let kotlin_captures = kotlin
        .highlight_visible_range(&registry, kotlin_source, 0..kotlin_source.len())
        .expect("kotlin highlights");
    assert!(
        kotlin_captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );
    assert!(
        kotlin_captures
            .iter()
            .any(|capture| capture.style_key == "function")
    );

    let mut nix = SyntaxSession::new();
    let nix_source =
        "{ pkgs, ... }: let name = \"hunk\"; in pkgs.mkShell { buildInputs = [ pkgs.git ]; }\n";
    nix.parse_for_path(&registry, Path::new("flake.nix"), nix_source)
        .expect("parse nix");
    let nix_captures = nix
        .highlight_visible_range(&registry, nix_source, 0..nix_source.len())
        .expect("nix highlights");
    assert!(
        nix_captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );
    assert!(
        nix_captures
            .iter()
            .any(|capture| capture.style_key == "variable")
    );
}

#[test]
fn phase_three_and_four_languages_parse_and_highlight_representative_tokens() {
    let registry = LanguageRegistry::builtin();

    let mut sql = SyntaxSession::new();
    let sql_source = "SELECT users.id FROM users WHERE users.active = true;\n";
    sql.parse_for_path(&registry, Path::new("schema.sql"), sql_source)
        .expect("parse sql");
    let sql_captures = sql
        .highlight_visible_range(&registry, sql_source, 0..sql_source.len())
        .expect("sql highlights");
    assert!(
        sql_captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );
    assert!(
        sql_captures
            .iter()
            .any(|capture| capture.style_key == "type")
    );

    let mut dockerfile = SyntaxSession::new();
    let dockerfile_source = "FROM rust:1.88\nLABEL version=\"1.0\"\nRUN cargo build --release\n";
    dockerfile
        .parse_for_path(&registry, Path::new("Dockerfile"), dockerfile_source)
        .expect("parse dockerfile");
    let dockerfile_captures = dockerfile
        .highlight_visible_range(&registry, dockerfile_source, 0..dockerfile_source.len())
        .expect("dockerfile highlights");
    assert!(
        dockerfile_captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );
    assert!(
        dockerfile_captures
            .iter()
            .any(|capture| capture.style_key == "string")
    );

    let mut markdown = SyntaxSession::new();
    let markdown_source = "# Hunk\n\n- Use `cargo test` with *care* and [docs](https://example.com).\n\n```rust\nfn main() {}\n```\n";
    markdown
        .parse_for_path(&registry, Path::new("README.md"), markdown_source)
        .expect("parse markdown");
    let markdown_captures = markdown
        .highlight_visible_range(&registry, markdown_source, 0..markdown_source.len())
        .expect("markdown highlights");
    assert!(
        markdown_captures
            .iter()
            .any(|capture| capture.style_key == "title")
    );
    assert!(
        markdown_captures
            .iter()
            .any(|capture| capture.style_key == "text.literal")
    );
    assert!(
        markdown_captures
            .iter()
            .any(|capture| capture.style_key == "emphasis")
    );
    assert!(
        markdown_captures
            .iter()
            .any(|capture| capture.style_key == "link_text")
    );
    assert!(
        markdown_captures
            .iter()
            .any(|capture| capture.style_key == "link_uri")
    );
    assert!(
        markdown_captures
            .iter()
            .any(|capture| capture.style_key == "keyword")
    );
}

#[test]
fn reused_session_clears_cached_tree_when_switching_languages() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();

    let rust_source = "fn main() {}\n";
    let rust_snapshot = session
        .parse_for_path(&registry, Path::new("main.rs"), rust_source)
        .expect("parse rust");
    assert_eq!(rust_snapshot.root_kind.as_deref(), Some("source_file"));

    let ts_source = "const answer = 42;\n";
    let ts_snapshot = session
        .parse_for_path(&registry, Path::new("main.ts"), ts_source)
        .expect("parse typescript after rust");
    assert_eq!(ts_snapshot.parse_status, ParseStatus::Ready);
    assert_eq!(ts_snapshot.root_kind.as_deref(), Some("program"));
}

#[test]
fn fold_candidates_cover_multiline_rust_blocks() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let source = "fn main() {\n    if true {\n        println!(\"hi\");\n    }\n}\n";

    session
        .parse_for_path(&registry, Path::new("main.rs"), source)
        .expect("parse");
    let folds = session.fold_candidates(&registry, source);

    assert!(
        folds
            .iter()
            .any(|fold| fold.start_line == 0 && fold.end_line >= 4)
    );
    assert!(
        folds
            .iter()
            .any(|fold| fold.start_line == 1 && fold.end_line >= 3)
    );
}

#[test]
fn semantic_tokens_override_syntax_captures_when_layers_merge() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let source = "fn greet(name: &str) {\n    println!(\"{name}\");\n}\n";

    session
        .parse_for_path(&registry, Path::new("main.rs"), source)
        .expect("parse");
    let syntax = session
        .highlight_visible_range(&registry, source, 0..source.len())
        .expect("syntax highlights");
    let semantic = semantic_token_captures(
        source,
        &[SemanticToken {
            range: TextRange::new(TextPosition::new(0, 9), TextPosition::new(0, 13)),
            kind: SemanticTokenKind::Parameter,
            modifiers: Vec::new(),
        }],
        0..source.len(),
    );
    let merged = merge_highlight_layers(&syntax, &semantic);

    let name_start = source.find("name").expect("name offset");
    let capture = merged
        .iter()
        .rev()
        .find(|capture| {
            capture.byte_range.start <= name_start && name_start < capture.byte_range.end
        })
        .expect("merged capture for semantic token");

    assert_eq!(capture.style_key, "variable.parameter");
}

#[test]
fn hover_and_definition_targets_map_the_symbol_under_cursor() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let source = "fn main() {\n    let answer = compute_value();\n}\n";

    session
        .parse_for_path(&registry, Path::new("main.rs"), source)
        .expect("parse");

    let hover = session
        .hover_target_at(source, TextPosition::new(1, 9))
        .expect("hover target");
    assert_eq!(hover.text, "answer");
    assert_eq!(
        hover.range,
        TextRange::new(TextPosition::new(1, 8), TextPosition::new(1, 14))
    );

    let definition = session
        .definition_target_at(source, TextPosition::new(1, 18))
        .expect("definition target");
    assert_eq!(definition.text, "compute_value");
    assert_eq!(
        definition.range,
        TextRange::new(TextPosition::new(1, 17), TextPosition::new(1, 30))
    );
}

#[test]
fn completion_context_tracks_identifier_prefix_and_replace_range() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let source = "fn main() {\n    answ\n}\n";

    session
        .parse_for_path(&registry, Path::new("main.rs"), source)
        .expect("parse");
    let completion = session
        .completion_context_at(
            source,
            TextPosition::new(1, 8),
            CompletionTriggerKind::Invoked,
        )
        .expect("completion context");

    assert_eq!(completion.prefix, "answ");
    assert_eq!(
        completion.replace_range,
        TextRange::new(TextPosition::new(1, 4), TextPosition::new(1, 8))
    );
}

#[test]
fn markdown_frontmatter_html_and_fences_keep_embedded_highlighting() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let source =
        "---\ntitle: Hunk\n---\n\n```rust\nconst ANSWER: u32 = 42;\n```\n\n<div>hi</div>\n";

    session
        .parse_for_path(&registry, Path::new("README.md"), source)
        .expect("parse markdown with injections");
    let captures = session
        .highlight_visible_range(&registry, source, 0..source.len())
        .expect("markdown highlights");

    let title_offset = source.find("title").expect("yaml property");
    let hunk_offset = source.find("Hunk").expect("yaml string");
    let const_offset = source.find("const").expect("rust keyword");
    let div_offset = source.find("div").expect("html tag");

    assert!(captures.iter().any(|capture| {
        capture.style_key == "property"
            && capture.byte_range.start <= title_offset
            && title_offset < capture.byte_range.end
    }));
    assert!(captures.iter().any(|capture| {
        capture.style_key == "string"
            && capture.byte_range.start <= hunk_offset
            && hunk_offset < capture.byte_range.end
    }));
    assert!(captures.iter().any(|capture| {
        capture.style_key == "keyword"
            && capture.byte_range.start <= const_offset
            && const_offset < capture.byte_range.end
    }));
    assert!(captures.iter().any(|capture| {
        capture.style_key == "tag"
            && capture.byte_range.start <= div_offset
            && div_offset < capture.byte_range.end
    }));
}

#[test]
fn markdown_reparse_after_text_edits_keeps_highlighting_working() {
    let registry = LanguageRegistry::builtin();
    let mut session = SyntaxSession::new();
    let mut source = "# Hunk\n\n- item\n\n```rust\nfn main() {}\n```\n".to_string();

    for inserted in ["\n", "G", "\n    indented"] {
        session
            .parse_for_path(&registry, Path::new("README.md"), source.as_str())
            .expect("parse markdown");
        session
            .highlight_visible_range(&registry, source.as_str(), 0..source.len())
            .expect("markdown highlights before edit");

        source.push_str(inserted);

        session
            .parse_for_path(&registry, Path::new("README.md"), source.as_str())
            .expect("reparse markdown after edit");
        let captures = session
            .highlight_visible_range(&registry, source.as_str(), 0..source.len())
            .expect("markdown highlights after edit");

        assert!(
            !captures.is_empty(),
            "markdown highlights should remain available after inserting {:?}",
            inserted
        );
    }
}
