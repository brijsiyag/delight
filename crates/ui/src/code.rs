//! Highlighted code for the built-in tools' output: tree-sitter with each grammar's
//! own highlight query, drawn in the theme's syntax colours (`code` feature, so only
//! what shows code builds tree-sitter). `.env` files have no grammar here: their few
//! rules are read line by line ([`env_highlights`]).
//!
//! [`Code::new`] does the work: run it on a background thread (it parses the whole
//! text), then draw the result with [`CodeBlock`].

use std::ops::Range;
use std::sync::{Arc, LazyLock};

use gpui::{
    App, HighlightStyle, IntoElement, ParentElement, RenderOnce, SharedString, StyledText, Styled, Window, div,
    prelude::FluentBuilder, px,
};
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

use crate::{ActiveTheme, v_flex};

/// Longer text shows only its first lines (GPUI lays out every line drawn): copy the
/// output to get all of it.
const MAX_LINES: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Json,
    Yaml,
    /// `KEY=value` lines, as in a `.env` file.
    Env,
}

/// What a highlighted range is; [`CodeBlock`] picks its colour from the theme when
/// drawing, so highlighted text follows appearance changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Property,
    String,
    Number,
    Constant,
    Comment,
    Type,
    Keyword,
    Punctuation,
}

/// The capture names the grammars' queries use, and what each one is. A capture takes
/// the longest name here that's a prefix of it (`constant.builtin` → `constant`).
const CAPTURES: [(&str, Token); 12] = [
    ("string.special.key", Token::Property),
    ("property", Token::Property),
    ("string", Token::String),
    ("escape", Token::Constant),
    ("number", Token::Number),
    ("boolean", Token::Constant),
    ("constant", Token::Constant),
    ("label", Token::Constant),
    ("comment", Token::Comment),
    ("type", Token::Type),
    ("attribute", Token::Keyword),
    ("punctuation", Token::Punctuation),
];

fn configuration(language: tree_sitter::Language, name: &str, highlights: &str) -> HighlightConfiguration {
    let mut config = HighlightConfiguration::new(language, name, highlights, "", "")
        .expect("the grammar's own highlight query is valid");
    config.configure(&CAPTURES.map(|(name, _)| name));
    config
}

/// tree-sitter-highlight gives a node the *last* pattern that matches it; the JSON
/// query lists its key pattern first (for editors where the first wins), so it's
/// repeated at the end, or keys would be plain strings.
static JSON: LazyLock<HighlightConfiguration> = LazyLock::new(|| {
    let query = format!("{}\n(pair key: (_) @string.special.key)", tree_sitter_json::HIGHLIGHTS_QUERY);
    configuration(tree_sitter_json::LANGUAGE.into(), "json", &query)
});
static YAML: LazyLock<HighlightConfiguration> =
    LazyLock::new(|| configuration(tree_sitter_yaml::LANGUAGE.into(), "yaml", tree_sitter_yaml::HIGHLIGHTS_QUERY));

/// The highlighted ranges of `text`, innermost last.
pub fn highlight(language: Language, text: &str) -> Vec<(Range<usize>, Token)> {
    let config = match language {
        Language::Json => &*JSON,
        Language::Yaml => &*YAML,
        Language::Env => return env_highlights(text),
    };
    let mut highlighter = Highlighter::new();
    let Ok(events) = highlighter.highlight(config, text.as_bytes(), None, |_| None) else {
        return Vec::new();
    };
    let mut ranges = Vec::new();
    let mut open: Vec<Token> = Vec::new();
    for event in events {
        match event {
            Ok(HighlightEvent::HighlightStart(highlight)) => open.push(CAPTURES[highlight.0].1),
            Ok(HighlightEvent::HighlightEnd) => {
                open.pop();
            }
            Ok(HighlightEvent::Source { start, end }) => {
                if let Some(token) = open.last() {
                    ranges.push((start..end, *token));
                }
            }
            Err(_) => break,
        }
    }
    ranges
}

/// A `.env` file's ranges: comments, `export`, names, `=`, and values (quoted ones as
/// strings, numbers and booleans as such).
fn env_highlights(text: &str) -> Vec<(Range<usize>, Token)> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        let indent = content.len() - content.trim_start().len();
        let body = &content[indent..];
        let at = |from: usize, to: usize| start + indent + from..start + indent + to;
        if body.starts_with('#') {
            ranges.push((at(0, body.len()), Token::Comment));
        } else if let Some(eq) = body.find('=') {
            let mut name_start = 0;
            if let Some(rest) = body.strip_prefix("export ") {
                ranges.push((at(0, 6), Token::Keyword));
                name_start = body.len() - rest.len();
            }
            ranges.push((at(name_start, eq), Token::Property));
            ranges.push((at(eq, eq + 1), Token::Punctuation));
            let value = &body[eq + 1..];
            if !value.is_empty() {
                let token = match value {
                    v if v.starts_with(['"', '\'']) => Token::String,
                    "true" | "false" => Token::Constant,
                    v if v.parse::<f64>().is_ok() => Token::Number,
                    _ => Token::String,
                };
                ranges.push((at(eq + 1, body.len()), token));
            }
        }
        start += line.len();
    }
    ranges
}

/// Text ready to draw: its first [`MAX_LINES`] lines, highlighted. Cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct Code {
    text: SharedString,
    highlights: Arc<[(Range<usize>, Token)]>,
    /// Lines left out after [`MAX_LINES`].
    hidden_lines: usize,
}

impl Code {
    /// Highlights `text` (plain text if `language` is `None`). It parses the text:
    /// call it on a background thread.
    pub fn new(language: Option<Language>, text: &str) -> Self {
        let (shown, hidden_lines) = match text.match_indices('\n').nth(MAX_LINES - 1) {
            Some((end, _)) => (&text[..end], text[end + 1..].lines().count()),
            None => (text, 0),
        };
        let highlights = language.map(|language| highlight(language, shown)).unwrap_or_default();
        Self { text: shown.to_string().into(), highlights: highlights.into(), hidden_lines }
    }

    /// The text shown.
    pub fn text(&self) -> &SharedString {
        &self.text
    }
}

/// A block of monospaced, highlighted text on a subtle fill.
#[derive(IntoElement)]
pub struct CodeBlock {
    code: Code,
}

impl CodeBlock {
    pub fn new(code: Code) -> Self {
        Self { code }
    }
}

impl RenderOnce for CodeBlock {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        let syntax = t.syntax();
        let color = |token: Token| match token {
            Token::Property => syntax.property,
            Token::String => syntax.string,
            Token::Number => syntax.number,
            Token::Constant => syntax.constant,
            Token::Comment => syntax.comment,
            Token::Type => syntax.type_,
            Token::Keyword => syntax.keyword,
            Token::Punctuation => syntax.punctuation,
        };
        let highlights = self
            .code
            .highlights
            .iter()
            .map(|(range, token)| (range.clone(), HighlightStyle { color: Some(color(*token)), ..Default::default() }));
        let text = StyledText::new(self.code.text.clone()).with_highlights(highlights);
        let hidden = self.code.hidden_lines;
        v_flex()
            .gap(px(8.))
            .px(px(12.))
            .py(px(10.))
            .rounded(t.radius)
            .bg(t.fill_subtle())
            .font_family(t.mono_font.clone())
            .text_size(t.mono_size())
            .text_color(t.text)
            .child(div().child(text))
            .when(hidden > 0, |block| {
                block.child(
                    div()
                        .text_color(t.text_faint)
                        .child(format!("… {hidden} more lines. Copy the output to get all of it.")),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(language: Language, text: &str) -> Vec<(&str, Token)> {
        highlight(language, text).into_iter().map(|(range, token)| (&text[range], token)).collect()
    }

    #[test]
    fn json_keys_values_and_constants() {
        let found = tokens(Language::Json, r#"{"a": "b", "n": -1.5, "t": true, "z": null}"#);
        assert!(found.contains(&("\"a\"", Token::Property)));
        assert!(found.contains(&("\"b\"", Token::String)));
        assert!(found.contains(&("-1.5", Token::Number)));
        assert!(found.contains(&("true", Token::Constant)));
        assert!(found.contains(&("null", Token::Constant)));
    }

    #[test]
    fn yaml_keys_and_values() {
        let found = tokens(Language::Yaml, "name: delight\ncount: 3\n# note\n");
        assert!(found.contains(&("name", Token::Property)));
        assert!(found.contains(&("delight", Token::String)));
        assert!(found.contains(&("3", Token::Number)));
        assert!(found.contains(&("# note", Token::Comment)));
    }

    #[test]
    fn env_names_values_and_comments() {
        let found = tokens(Language::Env, "# db\nexport HOST=localhost\nPORT=5432\nNAME=\"a b\"\nDEBUG=false\nEMPTY=\n");
        assert!(found.contains(&("# db", Token::Comment)));
        assert!(found.contains(&("export", Token::Keyword)));
        assert!(found.contains(&("HOST", Token::Property)));
        assert!(found.contains(&("localhost", Token::String)));
        assert!(found.contains(&("5432", Token::Number)));
        assert!(found.contains(&("\"a b\"", Token::String)));
        assert!(found.contains(&("false", Token::Constant)));
        assert!(found.contains(&("EMPTY", Token::Property)));
    }

    #[test]
    fn long_text_keeps_its_first_lines() {
        let text = "1\n".repeat(MAX_LINES + 5);
        let code = Code::new(None, &text);
        assert_eq!(code.text.lines().count(), MAX_LINES);
        assert_eq!(code.hidden_lines, 5);
    }
}
