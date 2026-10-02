//! A converting tool's result (JSON, YAML, Base64, .env, JWT). [`Conversion`] is what
//! a conversion produced: plain data, easy to test. [`Output`] is that, highlighted
//! and ready to draw: the code alone, with no caption, as the tool's own controls (its
//! tabs, its title in the launcher) already say what it is.

use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Styled, div, px};

use crate::code::{Code, CodeBlock, Language};
use crate::{ActiveTheme, Caption, h_flex, v_flex};

/// What a conversion produced.
#[derive(Debug, Clone, PartialEq)]
pub enum Conversion {
    /// Nothing to convert (no input).
    Empty,
    Text {
        /// `None`: plain text.
        language: Option<Language>,
        text: String,
    },
    Failed {
        message: String,
        /// Where the input breaks: 1-based line and column.
        position: Option<(usize, usize)>,
    },
}

impl Conversion {
    pub fn text(language: Language, text: String) -> Self {
        Conversion::Text { language: Some(language), text }
    }

    pub fn plain(text: String) -> Self {
        Conversion::Text { language: None, text }
    }

    pub fn failed(message: impl Into<String>, position: Option<(usize, usize)>) -> Self {
        Conversion::Failed { message: message.into(), position }
    }
}

/// A conversion ready to draw.
#[derive(Clone, Default)]
pub enum Output {
    #[default]
    Empty,
    Text {
        /// All of it, for copying ([`Code`] may show only its first lines).
        text: SharedString,
        code: Code,
    },
    Failed {
        message: SharedString,
        /// The input's lines up to where it breaks, marked with a caret.
        near: Option<Code>,
    },
}

impl Output {
    /// Highlights the result, or quotes `input` around the error. It parses the text:
    /// call it on a background thread.
    pub fn new(conversion: Conversion, input: &str) -> Self {
        match conversion {
            Conversion::Empty => Output::Empty,
            Conversion::Text { language, text } => Output::Text { code: Code::new(language, &text), text: text.into() },
            Conversion::Failed { message, position } => Output::Failed {
                message: message.into(),
                near: position.filter(|(line, _)| *line > 0).map(|(line, column)| near(input, line, column)),
            },
        }
    }

    /// The result's text, for copying.
    pub fn text(&self) -> Option<&SharedString> {
        match self {
            Output::Text { text, .. } => Some(text),
            _ => None,
        }
    }

    /// The result: the code, or the error and where the input breaks.
    pub fn render(&self, cx: &App) -> AnyElement {
        match self {
            Output::Empty => div().into_any_element(),
            Output::Text { code, .. } => CodeBlock::new(code.clone()).into_any_element(),
            Output::Failed { message, near } => v_flex()
                .gap(px(8.))
                .child(error(message.clone(), cx))
                .children(
                    near.clone()
                        .map(|near| v_flex().gap(px(8.)).child(Caption::new("Near")).child(CodeBlock::new(near))),
                )
                .into_any_element(),
        }
    }
}

/// Up to three lines of `text` ending at `line`, with a caret under `column`.
fn near(text: &str, line: usize, column: usize) -> Code {
    let first = line.saturating_sub(3);
    let mut excerpt = String::new();
    for (i, l) in text.lines().enumerate().skip(first).take(line - first) {
        excerpt.push_str(&format!("{:>5} │ {l}\n", i + 1));
    }
    excerpt.push_str(&format!("{:>5} │ {}^", "", " ".repeat(column.saturating_sub(1))));
    Code::new(None, &excerpt)
}

/// An error line on its tint.
pub fn error(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    let t = cx.theme();
    let color = t.error;
    h_flex()
        .px(px(12.))
        .py(px(8.))
        .rounded(t.radius)
        .bg(t.tint(color))
        .text_color(color)
        .text_size(t.text_size_small())
        .child(text.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_quotes_the_lines_up_to_the_error_with_a_caret() {
        let text = "{\n  \"a\": 1,\n  \"b\" 2\n}";
        let Output::Failed { near: Some(near), .. } = Output::new(Conversion::failed("bad", Some((3, 7))), text) else {
            panic!("expected an excerpt");
        };
        assert_eq!(
            near.text().as_ref(),
            "    1 │ {\n    2 │   \"a\": 1,\n    3 │   \"b\" 2\n      │       ^"
        );
    }
}
