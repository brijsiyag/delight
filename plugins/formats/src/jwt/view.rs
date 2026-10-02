//! The two tools. JWT: the secret's field, the signature verified as it's typed, then
//! the header as JSON, the registered claims as rows and the other claims as JSON. JSON → JWT: the algorithm and the secret's field, and the token,
//! signed again as either changes.
//!
//! Each field keeps its secret until the plugin stops, so the next token is verified, or
//! signed, with it too.

use std::time::{SystemTime, UNIX_EPOCH};

use delight_plugin_api::{Action, Actions, Input, Shortcut, Tool, host};
use delight_ui::code::{Code, CodeBlock, Language};
use delight_ui::conversion::error;
use delight_ui::{ActiveTheme as _, Caption, Group, SegmentedControl, Theme, h_flex, v_flex};
use gpui::{
    AnyElement, App, ClipboardItem, Context, FocusHandle, Hsla, InteractiveElement, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use serde_json::Value;

use super::{Algorithm, Claim, Jwt, Tone, claims, decode, sign};
use crate::field::Field;

fn pretty(value: &impl serde::Serialize) -> String {
    serde_json::to_string_pretty(value).expect("serializing a Value can't fail")
}

fn copy(label: &str, text: String, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    host(cx).toast(format!("{label} — copied to clipboard"), cx);
}

// ---------------------------------------------------------------------------
// JWT: read and verify
// ---------------------------------------------------------------------------

/// A token as shown: decoded once, when the input changes.
struct Shown {
    jwt: Jwt,
    /// The header, highlighted.
    header: Code,
    rows: Vec<Claim>,
    /// The other claims, or a payload that isn't an object, highlighted.
    others: Option<Code>,
}

#[derive(Default)]
pub struct JwtView {
    token: Option<Result<Shown, String>>,
    /// What the secret's field has.
    secret: String,
    field: Field,
    /// Where the pane is scrolled, kept while the tool is hidden.
    scroll: ScrollHandle,
}

/// The footer's actions.
#[derive(Actions)]
pub enum JwtAction {
    CopyPayload,
    CopyHeader,
}

impl JwtView {
    fn jwt(&self) -> Option<&Jwt> {
        match &self.token {
            Some(Ok(shown)) => Some(&shown.jwt),
            _ => None,
        }
    }

    /// Whether the secret made the token's signature; `None` without a secret, or for a
    /// token a secret can't verify.
    fn verified(&self) -> Option<bool> {
        if self.secret.is_empty() {
            return None;
        }
        self.jwt()?.verify(&self.secret)
    }
}

impl Tool for JwtView {
    type Action = JwtAction;

    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
        let text = input.text.trim();
        self.token = (!text.is_empty()).then(|| {
            let jwt = decode(text)?;
            let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
            let (rows, others) = match &jwt.payload {
                Value::Object(payload) => {
                    let (rows, others) = claims(payload, now, host(cx).utc_offset_seconds(cx));
                    (rows, (!others.is_empty()).then(|| pretty(&others)))
                }
                other => (Vec::new(), Some(pretty(other))),
            };
            let others = others.map(|json| Code::new(Some(Language::Json), &json));
            let header = Code::new(Some(Language::Json), &pretty(&jwt.header));
            Ok(Shown { jwt, header, rows, others })
        });
        cx.notify();
    }

    fn list_actions(&self, _: &App) -> Vec<Action<JwtAction>> {
        if self.jwt().is_none() {
            return Vec::new();
        }
        vec![
            Action::new(JwtAction::CopyPayload, "Copy payload", Shortcut::Enter),
            Action::new(JwtAction::CopyHeader, "Copy header", Shortcut::CmdEnter),
        ]
    }

    fn perform_action(&mut self, action: JwtAction, cx: &mut Context<Self>) {
        let Some(jwt) = self.jwt() else { return };
        match action {
            JwtAction::CopyPayload => copy("Copy payload", pretty(&jwt.payload), cx),
            JwtAction::CopyHeader => copy("Copy header", pretty(&jwt.header), cx),
        }
    }
}

impl Render for JwtView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shown = match &self.token {
            None => return v_flex().size_full(),
            Some(Err(message)) => return v_flex().size_full().child(error(message.clone(), cx)),
            Some(Ok(shown)) => shown,
        };
        let verified = self.verified();
        // The secret's field, above the scrolling pane (see `field`), for a token a secret signs.
        let secret = shown.jwt.hmac().is_some().then(|| {
            self.field.render(
                "Secret, to verify the signature",
                |this: &mut Self, secret, _| this.secret = secret,
                window,
                cx,
            )
        });
        let t = cx.theme();
        let verified = verified.map(|ok| match ok {
            true => notice("Signature verified with this secret", t.success, t),
            false => notice("The signature doesn't match this secret", t.error, t),
        });
        let header = section("Header", CodeBlock::new(shown.header.clone()));
        let rows = (!shown.rows.is_empty())
            .then(|| section("Claims", Group::new().children(shown.rows.iter().map(|claim| claim_row(claim, t)))));
        let others = shown.others.clone().map(|code| {
            let title = if shown.jwt.payload.is_object() { "Other claims" } else { "Payload" };
            section(title, CodeBlock::new(code))
        });
        let details = v_flex()
            .id("jwt")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .gap(px(14.))
            .children(verified)
            .child(header)
            .children(rows)
            .children(others);
        v_flex().size_full().gap(px(14.)).children(secret).child(details)
    }
}

/// A part of the token under its caption.
fn section(title: &'static str, content: impl IntoElement) -> impl IntoElement {
    v_flex().gap(px(6.)).child(div().px(px(2.)).child(Caption::new(title))).child(content)
}

fn notice(text: &'static str, color: Hsla, t: &Theme) -> impl IntoElement {
    div().px(px(12.)).py(px(8.)).rounded(t.radius).bg(t.tint(color)).text_color(color).text_size(t.text_size_small()).child(text)
}

/// A claim's label, its value (in its tone's colour), and how long ago or until.
fn claim_row(claim: &Claim, t: &Theme) -> AnyElement {
    let color = match claim.tone {
        Tone::Plain => t.text,
        Tone::Good => t.success,
        Tone::Bad => t.error,
    };
    h_flex()
        .items_start()
        .gap(px(12.))
        .px(px(12.))
        .py(px(7.))
        .text_size(t.text_size_small())
        .child(div().w(px(96.)).flex_shrink_0().text_color(t.text_muted).child(claim.label))
        .child(div().flex_1().min_w(px(0.)).font_family(t.mono_font.clone()).text_color(color).child(claim.value.clone()))
        .children(claim.hint.clone().map(|hint| div().flex_shrink_0().text_color(t.text_faint).child(hint)))
        .into_any_element()
}

// ---------------------------------------------------------------------------
// JSON → JWT: sign
// ---------------------------------------------------------------------------

pub struct SignView {
    /// The algorithm tabs' focus: a Tab stop, where ← / → switch them.
    algorithms_focus: FocusHandle,
    input: String,
    algorithm: Algorithm,
    /// What the secret's field has.
    secret: String,
    field: Field,
    /// The token; `None` without JSON or a secret.
    token: Option<Result<String, String>>,
}

/// The footer's action.
#[derive(Actions)]
pub enum SignAction {
    CopyToken,
}

impl SignView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            algorithms_focus: cx.focus_handle().tab_stop(true),
            input: String::new(),
            algorithm: Algorithm::default(),
            secret: String::new(),
            field: Field::default(),
            token: None,
        }
    }

    fn sign(&mut self) {
        self.token = (!self.input.is_empty() && !self.secret.is_empty()).then(|| sign(&self.input, self.algorithm, &self.secret));
    }
}

impl Tool for SignView {
    type Action = SignAction;

    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
        self.input = input.text.trim().to_string();
        self.sign();
        cx.notify();
    }

    fn list_actions(&self, _: &App) -> Vec<Action<SignAction>> {
        match self.token {
            Some(Ok(_)) => vec![Action::new(SignAction::CopyToken, "Copy token", Shortcut::Enter)],
            _ => Vec::new(),
        }
    }

    fn perform_action(&mut self, action: SignAction, cx: &mut Context<Self>) {
        match action {
            SignAction::CopyToken => {
                if let Some(Ok(token)) = &self.token {
                    copy("Copy token", token.clone(), cx);
                }
            }
        }
    }
}

impl Render for SignView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let algorithms = SegmentedControl::new("jwt-algorithm")
            .focus(&self.algorithms_focus)
            .options(Algorithm::ALL.map(Algorithm::name))
            .selected(Algorithm::ALL.iter().position(|algorithm| *algorithm == self.algorithm).unwrap_or(0))
            .on_change(cx.listener(|this, index: &usize, _, cx| {
                this.algorithm = Algorithm::ALL[*index];
                this.sign();
                cx.notify();
            }));
        let secret = self.field.render(
            "Secret to sign with",
            |this: &mut Self, secret, _| {
                this.secret = secret;
                this.sign();
            },
            window,
            cx,
        );
        let t = cx.theme();
        let token: AnyElement = match &self.token {
            Some(Ok(token)) => CodeBlock::new(Code::new(None, token)).into_any_element(),
            Some(Err(message)) => error(SharedString::from(message.clone()), cx).into_any_element(),
            None => div().px(px(2.)).text_size(t.text_size_small()).text_color(t.text_faint).child("Type a secret to sign the JSON with.").into_any_element(),
        };
        v_flex()
            .size_full()
            .gap(px(14.))
            .child(h_flex().gap(px(10.)).child(algorithms).child(secret))
            .child(div().id("sign").flex_1().min_h(px(0.)).overflow_y_scroll().child(token))
    }
}
