# Getting started

This page sets up a plugin crate and writes **Case**, a plugin with one tool that changes text to
UPPER, lower, Title or snake_case: type `case hello world` and it opens, type any line of words and
it waits under *Other Matches*. Then it builds, installs and tests it.

## What you need

- **A Mac with Delight installed**, to try the plugin in.
- **Rust, through [rustup](https://rustup.rs).** The `rust-toolchain.toml` below makes rustup fetch
  the toolchain Delight pins (Rust 1.95.0 and its `wasm32-wasip2` target).
- **About 4 GB of disk** for the first build: GPUI is large.
- A **WASI SDK** only if your plugin compiles C code ([C code](#c-code)).

## Create the crate

```sh
cargo new --lib case
cd case
mkdir assets
```

```text
case/
├── Cargo.toml
├── Cargo.lock            commit it: it pins GPUI's commit
├── rust-toolchain.toml
├── assets/
│   └── icon.svg          the plugin's icon
└── src/
    └── lib.rs
```

## `Cargo.toml`

```toml
[package]
name = "case"
version = "0.1.0"          # the plugin's version: Delight shows it and updates by it
edition = "2024"

[lib]
crate-type = ["cdylib"]    # a WebAssembly component, not a Rust library

[dependencies]
# Delight's plugin API, from the release of Delight you build for: its tag.
delight-plugin-api = { git = "https://github.com/brijsiyag/delight.git", tag = "v0.0.7" }
# GPUI, named exactly like this: any other spelling links a second GPUI.
gpui = { git = "https://github.com/zed-industries/zed.git", branch = "gpui-multi-root-embedded-rebased", version = "=0.2.2", default-features = false }

# A smaller, faster plugin.
[profile.release]
lto = true
opt-level = "s"
strip = true

# The async-task GPUI's own workspace builds with: patches don't reach through a git dependency.
[patch.crates-io]
async-task = { git = "https://github.com/smol-rs/async-task.git", rev = "b4486cd71e4e94fbda54ce6302444de14f4d190e" }
```

- **`delight-plugin-api`** is what a plugin is written with: the macros, the `Plugin` and `Tool`
  traits, `host` and the app's theme. It comes from a release's tag, and
  `Cargo.lock` pins the tag's commit. [Publishing](publishing.md#why-git-and-not-cratesio) says why it is git.
- **`gpui`** is what the plugin draws its UI with: all of it, its own. It must be the GPUI the
  plugin API is built on. Copy the line as it is: crates.io's
  `gpui`, or any other spelling, links a second GPUI whose views can't go into Delight's.
- **The `async-task` patch** is the one Zed builds GPUI with, and Delight too. Cargo applies patches
  only from the workspace being built, so a plugin repeats it.
- **Pin your own dependencies exactly** (`=1.2.3`). Where GPUI links a crate too (`resvg`,
  `image`, `regex`), use GPUI's version, or Cargo builds both.

## `rust-toolchain.toml`

```toml
[toolchain]
channel = "1.95.0"
profile = "minimal"
components = ["clippy"]
targets = ["wasm32-wasip2"]
```

## The icon

`assets/icon.svg`: a **square, full-colour SVG with its own background**, drawn as it is in light
and dark ([Icons](tools.md#icons)).

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <rect width="64" height="64" rx="14" fill="#5856D6"/>
  <g fill="none" stroke="#FFFFFF" stroke-width="4" stroke-linecap="round" stroke-linejoin="round">
    <path d="M12 46 22 18 32 46M16 36h12"/>
    <circle cx="44" cy="39" r="7"/>
    <path d="M51 32v14"/>
  </g>
</svg>
```

## Write the plugin

The blocks below, one after another, are the whole of `src/lib.rs`.

### 1. Say what the plugin is

```rust
use delight_plugin_api::{Action, Actions, AnyTool, Detection, Input, Operations, Plugin, Shortcut, Tool, host, plugin, theme};
use gpui::prelude::*;
use gpui::{App, ClipboardItem, Context, Hsla, Window, div, px};

#[plugin(
    id = "dev.example.case",
    name = "Case",
    description = "Change text to UPPER, lower, Title or snake_case.",
    author = "You",
    icon = "assets/icon.svg",
    tips = ["case <text> changes its case"],
)]
struct Case;
```

`#[plugin]` makes `Case` the plugin. Its properties are checked as the crate compiles and stored in
the `.wasm`, where Delight reads them without running the plugin. The `id` names everything Delight
keeps for the plugin, so it never changes; the version is the crate's. [The
manifest](tools.md#the-manifest) lists every property.

### 2. Name its tools

```rust
#[derive(Operations)]
enum CaseOperation {
    #[operation(id = "change-case", title = "Change Case")]
    ChangeCase,
}
```

Each variant is one tool (an *operation*). Delight stores its `id` (the input history, the tool
that was picked), so it is written out and kept.

### 3. Implement `Plugin`

```rust
impl Plugin for Case {
    type Operation = CaseOperation;

    fn new(_cx: &mut App) -> Self {
        Case
    }

    // Runs on every change of the input: keep it quick.
    fn detect(&mut self, input: &Input, _cx: &mut App) -> Vec<Detection<CaseOperation>> {
        confidence(&input.text)
            .map(|confidence| Detection::new(CaseOperation::ChangeCase, confidence))
            .into_iter()
            .collect()
    }

    fn open_tool(&mut self, operation: CaseOperation, cx: &mut App) -> AnyTool {
        match operation {
            CaseOperation::ChangeCase => cx.new(|_| CaseTool::default()).into(),
        }
    }
}
```

`new` runs once, when Delight starts the plugin. `detect` answers which tools fit the input, and
how well. `open_tool` builds a tool the first time it is picked; Delight keeps it, with its state.

### 4. Decide how well an input fits

```rust
/// How well the input fits: asked for by name, or any one line of words.
fn confidence(input: &str) -> Option<f32> {
    if input.starts_with("case ") && !text_of(input).is_empty() {
        Some(0.95)
    } else if !input.contains('\n') && input.len() <= 200 && input.chars().any(char::is_alphabetic) {
        Some(0.2)
    } else {
        None
    }
}

/// The text to change: what follows `case `, or the whole input.
fn text_of(input: &str) -> &str {
    input.strip_prefix("case ").unwrap_or(input).trim()
}
```

`case hello` starts with Case's own keyword, so it is certainly for Case: 0.95, and Case opens at
once. Any other line only *might* be: 0.2 offers Case under *Other Matches* without getting in the
way of tools that are surer. Above 0.8 only when the input is certainly yours ([How sure to
be](concepts.md#how-sure-to-be)).

### 5. The conversion

Plain functions, apart from the UI: they are what the tests check.

```rust
#[derive(Clone, Copy, PartialEq, Default)]
enum Mode {
    #[default]
    Upper,
    Lower,
    Title,
    Snake,
}

const MODES: [(Mode, &str); 4] =
    [(Mode::Upper, "UPPER"), (Mode::Lower, "lower"), (Mode::Title, "Title"), (Mode::Snake, "snake_case")];

fn convert(text: &str, mode: Mode) -> String {
    match mode {
        Mode::Upper => text.to_uppercase(),
        Mode::Lower => text.to_lowercase(),
        Mode::Title => text.split(' ').map(capitalize).collect::<Vec<_>>().join(" "),
        Mode::Snake => text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
            .join("_"),
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect(),
        None => String::new(),
    }
}
```

### 6. The tool's state and actions

```rust
#[derive(Default)]
struct CaseTool {
    text: String,
    mode: Mode,
}

impl CaseTool {
    fn result(&self) -> String {
        convert(&self.text, self.mode)
    }
}

#[derive(Actions)]
enum CaseAction {
    Copy,
    UseAsInput,
}
```

A tool is a GPUI entity: its state is ordinary fields.

### 7. Implement `Tool`

```rust
impl Tool for CaseTool {
    type Action = CaseAction;

    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
        self.text = text_of(&input.text).to_string();
        cx.notify();
    }

    fn list_actions(&self, _cx: &App) -> Vec<Action<CaseAction>> {
        if self.text.is_empty() {
            return Vec::new();
        }
        vec![
            Action::new(CaseAction::Copy, "Copy", Shortcut::Enter).primary(),
            Action::new(CaseAction::UseAsInput, "Use as Input", Shortcut::CmdEnter),
        ]
    }

    fn perform_action(&mut self, action: CaseAction, cx: &mut Context<Self>) {
        let result = self.result();
        match action {
            CaseAction::Copy => {
                cx.write_to_clipboard(ClipboardItem::new_string(result));
                host(cx).toast("Copied to clipboard", cx);
            }
            CaseAction::UseAsInput => host(cx).set_input(result, cx),
        }
    }
}
```

- `cx.notify()` redraws the tool, and makes Delight ask for its actions again.
- The footer's keys are fixed: ↵, ⌘↵ and ⌥1–⌥9, the same in every tool.
- The clipboard is GPUI's own. `host(cx)` is the app: a toast in the footer, and `set_input`, which
  puts the result in the launcher's input for the tools that fit it.

### 8. Draw it

```rust
impl Render for CaseTool {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The app's theme, light or dark. It arrives a moment after the plugin starts: until
        // then, draw nothing rather than guess at colours.
        let Some(t) = theme(cx).cloned() else { return div() };
        let modes = MODES.map(|(mode, label)| {
            let picked = mode == self.mode;
            // Each background with the text colour that goes on it.
            let (background, text): (Hsla, Hsla) =
                if picked { (t.accent.into(), t.accent_text.into()) } else { (t.fill.into(), t.text.into()) };
            div()
                .id(label)
                .px(px(10.))
                .py(px(4.))
                .rounded(px(t.radius))
                .bg(background)
                .text_color(text)
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.mode = mode;
                    cx.notify();
                }))
        });
        let result = div()
            .p(px(12.))
            .rounded(px(t.radius))
            .bg(Hsla::from(t.surface))
            .border_1()
            .border_color(t.border)
            .font_family(t.mono_font.clone())
            .child(self.result());
        // The root sets the text's size and colour for everything in it.
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .text_size(px(t.text_size))
            .text_color(t.text)
            .child(div().flex().gap(px(4.)).children(modes))
            .child(result)
    }
}
```

Plain GPUI: the plugin draws all of its UI itself. **Every colour comes from the app's theme**
(`delight_plugin_api::theme`), and every background comes with the text colour made for it: the
picked mode is `accent_text` on `accent`, the others `text` on `fill`, the result `text` on
`surface`. Delight draws a plugin's text in the theme's text colour and size unless the plugin says
otherwise, and draws the view again when the theme changes. [Drawing the UI](ui.md#use-the-theme-always) has the rules.

### 9. Test the logic

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_by_name_or_one_line_of_words() {
        assert_eq!(confidence("case hello world"), Some(0.95));
        assert_eq!(confidence("hello world"), Some(0.2));
        assert_eq!(confidence("one\ntwo"), None);
        assert_eq!(confidence("12345"), None);
    }

    #[test]
    fn converts() {
        assert_eq!(convert("hello world", Mode::Upper), "HELLO WORLD");
        assert_eq!(convert("Hello World", Mode::Lower), "hello world");
        assert_eq!(convert("hello wORLD", Mode::Title), "Hello World");
        assert_eq!(convert("Hello, big World!", Mode::Snake), "hello_big_world");
    }
}
```

## Build and install

```sh
cargo test
cargo build --release --target wasm32-wasip2
```

The plugin is `target/wasm32-wasip2/release/case.wasm` (the crate's name, `-` turned into `_`).
In Delight's Settings, click **Install Plugin** under the list of plugins and pick it, or drop it on
the menu bar icon. Then type `case hello world`: Change Case opens with `HELLO WORLD`.

- **Always build `--release`.** A debug `.wasm` is hundreds of megabytes and slow to start.
- **Each change:** build, and install the new file again. It replaces the installed one and
  restarts only that plugin; its data, settings and secrets stay.
- **Use your own `target/` folder.** A development build of Delight loads every `.wasm` in its
  built-ins' `target` as a built-in.
- **A workspace of several plugins** with a native tool in it (a release script) lists only the
  plugins in `default-members`, so `--target wasm32-wasip2` doesn't build the tool.

## Test and debug

`cargo test` runs on the Mac, with no Delight: natively the plugin API compiles, `host(cx)` calls do
nothing or fail, and `theme(cx)` is `None`. So keep the logic in plain functions (detection,
parsing, building requests, reading responses) and test those, with awkward inputs and with inputs
other tools want (a JSON object, a URL, a sentence). Never test by sending keystrokes to the running
launcher: they go into whatever you are typing in.

Log with the [`log`](https://docs.rs/log) crate (`log::info!` and up: `debug!` and `trace!` are
dropped in a plugin). Delight writes each line to its log as `plugin::<name>`: *Open Logs* in the
menu bar.

[Pitfalls](pitfalls.md) has what goes wrong, and why.

## C code

Rust that compiles C (tree-sitter grammars, `ring`, some compression crates) needs the
[WASI SDK](https://github.com/WebAssembly/wasi-sdk)'s C compiler and library for WebAssembly.
Delight builds with `wasi-sdk-34`. Download it, unpack it, and tell the `cc` crate where it is:

```toml
# .cargo/config.toml
[env]
WASI_SDK_PATH = "/path/to/wasi-sdk-34.0-arm64-macos"
```

Only building needs it: people who install the plugin need nothing.
