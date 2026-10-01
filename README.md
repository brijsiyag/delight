# Delight

A launcher for macOS. Press **⌘⇧Space**, type or paste something, and Delight offers the tools that
fit it: format the JSON, look up the DNS, preview the SVG, open a service's dashboards. Every tool is
a **plugin**, so you can add your own.

- Built-in tools: **JSON**, **YAML ⇄ JSON**, **SVG preview**, **DNS lookup**.
- Plugins are single `.wasm` files. Each one shows its own interface and can only do what it asked
  permission for.
- macOS, Apple Silicon and Intel. Signed and notarised; updates itself.

## Install

Download `Delight-X.Y.Z.dmg` from the [latest release](https://github.com/brijsiyag/delight/releases/latest),
drag Delight to Applications and start it. It lives in the menu bar; **⌘⇧Space** shows and hides the
launcher (change it in Settings → General). Delight checks for updates once a day; *Check for Updates…*
in the menu bar does it now.

To add a plugin: **Settings → Plugins → Install…**, pick the `.wasm`. Delight shows what the plugin
says it can do (network, running programs) and asks before installing.

## Using it

- Type or paste. The tools on the left are ranked by how well they fit the input; the best one opens.
  **⌘1–⌘9** picks a tool, **↓** or **Tab** moves into the list, **→** into the tool.
- The footer shows the tool's actions and their keys. Every tool uses the same keys: **↵**, **⌘↵**
  and **⌥1 … ⌥9**.
- **Tab** accepts a completion from your input history; **⌃R** searches it. Turn it off in
  Settings → General.
- **⌘K** clears, **⌘,** opens Settings, **Esc** hides.
- Where things are stored: `~/Library/Application Support/Delight` (settings, installed plugins,
  input history, each plugin's data), secrets in the login Keychain, logs in `~/Library/Logs/Delight`
  (*Open Logs* in the menu bar).

The full behaviour is in [`docs/behaviour.md`](docs/behaviour.md).

## How it works

```text
Delight.app (native GPUI)
 ├─ launcher: input → detection → tool list → tool pane → footer actions
 ├─ settings window
 └─ runtime: one sandboxed WebAssembly instance per plugin (wasmtime)
        ▲  a small object protocol (crates/protocol)
        ▼
plugin.wasm: its own GPUI, drawn into a surface the launcher shows
```

- **The app is generic.** It owns the window, the input, the tool list, the footer, settings, history
  and updates. It knows nothing about JSON or DNS: **tools draw all of their own UI**.
- **A plugin is a WebAssembly component** (`wasm32-wasip2`) that runs its own GPUI, through
  [embedded_gpui](https://github.com/zed-industries/embedded_gpui), and hands the app a view. What it
  is (id, name, icon, tools, permissions, tips) is stored in the `.wasm` itself, so the app reads it
  without running the plugin.
- **Detection.** On every change of the input the app asks each plugin `detect(input)`, which returns
  the tools that fit with a confidence from 0 to 1. Tools at 0.5 or more are *Recommended*; the
  highest is selected. Then the app hands the input to the tool (`on_input_changed`), shows its
  footer actions (`list_actions`) and runs the one you pick (`perform_action`). It tells the tool
  when its view is shown or hidden (`on_shown`, `on_hidden`): when it's picked or another is, and
  when the launcher hides and comes back, which sends no input.
- **Sandbox and permissions.** A plugin sees its own data folder (`/data`) and nothing else. The
  network and running programs need a permission the plugin declares with a reason, which you see
  when installing. A plugin that fails is stopped on its own and the others carry on.
- **Versioning.** Plugins are built against the plugin API and carry its `major.minor`. The app runs
  a plugin of the same major and a minor no newer than its own; anything else is refused with a
  message, and needs a rebuild. **Before 1.0 a minor bump can break plugins**, so rebuild your plugins
  when you update the API.

The layout of the code is in [`docs/plan.md`](docs/plan.md).

## Writing a plugin

A plugin is a Rust `cdylib` crate.

### Which crates, and why they are git dependencies

Nothing Delight builds on is released on crates.io yet, so everything is taken from git, pinned:

| Crate | Where from | Pinned to |
|---|---|---|
| `delight-plugin-api`, `delight-ui` | this repository, `github.com/brijsiyag/delight` | a release commit (`rev`) |
| `gpui` | Zed's `gpui-multi-root-embedded-rebased` branch, `github.com/zed-industries/zed` | the commit in `Cargo.lock` (today `8c88a5c`), `version = "=0.2.2"` |
| `embedded_gpui` (the layer that lets a plugin run its own GPUI) | for now the fork `github.com/brijsiyag/embedded_gpui`, branch `delight`: upstream's `surfaces-as-roots` plus hidden surfaces and a compile cache (see [`docs/development.md`](docs/development.md), "The embedded_gpui fork"); back to `zed-industries/embedded_gpui` once upstream has them | a commit, taken in by `delight-plugin-api`, so a plugin never names it |

- **GPUI is the branch's, not crates.io's `gpui`.** Plugins and the app must use exactly the same
  GPUI, so a plugin names it exactly as above (branch and version), or Cargo links a second copy and
  the build fails or misbehaves. Copy the `gpui` line from this repository's `Cargo.toml`, and
  commit your `Cargo.lock`, which pins the commit.
- **The plugin API's `rev` is the version you build for.** Change it to the commit of a newer release
  and rebuild to move up; a plugin built for a newer minor than the app is refused.
- **This is temporary.** Once embedded_gpui is officially released, Delight and its plugin API move
  to the released crates, and plugins will depend on those versions instead of git commits. The
  plugin code itself doesn't change; the `Cargo.toml` lines do, and the plugins need rebuilding
  against that release.

```toml
# Cargo.toml
[lib]
crate-type = ["cdylib"]

[dependencies]
delight-plugin-api = { git = "https://github.com/brijsiyag/delight.git", rev = "<a release commit>" }
delight-ui = { git = "https://github.com/brijsiyag/delight.git", rev = "<the same commit>" }
# Named exactly like this (see above), or Cargo links a second GPUI.
gpui = { git = "https://github.com/zed-industries/zed.git", branch = "gpui-multi-root-embedded-rebased", version = "=0.2.2", default-features = false }
```

Pin dependencies exactly, and copy `rust-toolchain.toml` (Rust 1.95 with the `wasm32-wasip2` target)
from this repository. Then the plugin is three things:

```rust
use delight_plugin_api::{Action, Actions, AnyTool, Detection, Input, Operations, Plugin, Shortcut, Tool, host, plugin};
use gpui::{App, AppContext as _, Context, IntoElement, Render, Window, div, prelude::*};

// 1. The plugin: what it is. Everything here is checked when it compiles.
#[plugin(
    id = "dev.example.shout",              // letters, digits, . _ -
    name = "Shout",
    description = "Turns text into capitals",
    author = "You",
    icon = "assets/icon.svg",              // square, full colour, its own background
    tips = ["shout <text> makes capitals"] // what to type, not which keys; at most 5, 60 characters each
)]
struct Shout;

// 2. Its tools ("operations"), and how it recognises input for them.
#[derive(Operations)]
enum ShoutOperation {
    #[operation(id = "shout", title = "Shout")]
    Shout,
}

impl Plugin for Shout {
    type Operation = ShoutOperation;
    fn new(_cx: &mut App) -> Self { Shout }

    // Runs on every keystroke: keep it quick.
    fn detect(&mut self, input: &Input, _cx: &mut App) -> Vec<Detection<ShoutOperation>> {
        if input.text.starts_with("shout ") { vec![Detection::new(ShoutOperation::Shout, 1.0)] } else { vec![] }
    }

    fn open_tool(&mut self, _op: ShoutOperation, cx: &mut App) -> AnyTool {
        cx.new(|_| ShoutTool { text: String::new() }).into()
    }
}

// 3. A tool: a GPUI view, plus footer actions.
struct ShoutTool { text: String }

#[derive(Actions)]
enum ShoutAction { Copy }

impl Tool for ShoutTool {
    type Action = ShoutAction;
    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
        self.text = input.text.trim_start_matches("shout ").to_uppercase();
        cx.notify();
    }
    fn list_actions(&self, _: &App) -> Vec<Action<ShoutAction>> {
        vec![Action::new(ShoutAction::Copy, "Copy", Shortcut::Enter).primary()]
    }
    fn perform_action(&mut self, _: ShoutAction, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(self.text.clone()));
        host(cx).toast("Copied", cx);
        host(cx).hide(cx);
    }
}

impl Render for ShoutTool {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement { div().child(self.text.clone()) }
}
```

Build it and install the file from Settings → Plugins → Install…:

```sh
cargo build --release --target wasm32-wasip2
# target/wasm32-wasip2/release/<crate_name>.wasm
```

Working examples: the built-ins in [`plugins/`](plugins) (JSON, YAML, SVG, DNS). Unit-test your logic natively (`cargo test`): the API compiles on the Mac and does nothing there.

### Rules that shape a plugin

- **Action keys are fixed:** `Shortcut::Enter` (↵), `CmdEnter` (⌘↵), `Option(1..=9)` (⌥1–⌥9) or
  `ClickOnly`. Any other key is refused. The footer shows four actions; a destructive one should
  never be on plain ↵ (mark it `.attention()` or leave it on a click). An action marked `.hidden()`
  has its key and no button: for keys you want without spending one of the four.
- **Theme:** use `delight_ui`'s components and theme colours, not your own tokens; they follow
  light/dark automatically.
- **Settings:** return sections from `Plugin::settings_sections`; the app draws the card, title and
  footer and you draw the rows. Heights are declared by you. See
  [`docs/plugin-settings.md`](docs/plugin-settings.md).
- **Permissions** are declared in `#[plugin(permissions = [...])]`, each with a reason of at most
  100 characters: `Network("why")` and `Commands("why", programs = ["/bin/ps"])`. Programs must be an
  absolute path directly in `/bin`, `/sbin`, `/usr/bin` or `/usr/sbin`.

### Publishing updates

Plugins are published at a location: a URL such as a GitHub release's download address. Delight
defines the files there; how they are made is up to the plugins' repository, and any tool can write
them (Meesho's plugins use `cargo xtask publish` in github.com/Meesho/delight-plugins). Rust tools
can read and write them with `delight_manifest::Release` and `PluginList` (feature `files`).

For each plugin the location holds two files, named by its id: `<id>.wasm`, the plugin (at most
64 MiB), and `<id>.xml`, its manifest there (at most 1 MiB):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<plugin id="local.logs" version="0.0.5">
  <name>Logs</name>
  <description>Search logs with KQL on your Elasticsearch hosts.</description>
  <sha256>…the .wasm's SHA-256: 64 lowercase hex digits…</sha256>
</plugin>
```

`id` and `version` are the plugin's own (its `#[plugin(id)]`, and its crate's version, SemVer), and
the `.wasm` must carry the same; `name` (the id if left out) and `description` are shown when
picking what to install. Beside them, a list can give several plugins at the location in the same
form, each once, under any file name:

```xml
<plugins>
  <plugin id="local.calendar" version="0.0.5">…</plugin>
  <plugin id="local.logs" version="0.0.5">…</plugin>
</plugins>
```

One location holds any number of plugins. A plugin names only its location:

```rust
#[plugin(id = "local.logs", name = "Logs", icon = "assets/icon.svg",
         update = "https://github.com/Meesho/delight-plugins/releases/latest/download")]
```

Delight reads `<location>/<id>.xml` at launch and once a day, and downloads `<location>/<id>.wasm`
only when the version (SemVer, the crate's `version`) is newer than the one installed. It checks the
file's SHA-256, its id and its version before anything runs. An update that asks for no new
permissions installs by itself unless the plugin's page has "Update automatically" off, keeping the
plugin's data, secrets and settings; one that asks for more is shown like a new install, and waits
for a yes.

People install from a location too: Settings → the arrow beside "Install Plugin" → "From a Link…",
with a link to an XML file at the location: a list (every plugin in it, to pick from) or one
plugin's `<id>.xml`. Delight reads the file to tell which; its name doesn't matter. A copy installed
before you added `update` doesn't know where to look: install the new build once.

Whatever publishes them: bump a plugin's version for every change (Delight takes only a newer one),
put the `.wasm` there before the `.xml` that names it, and, with a `latest` location, attach every
plugin to every release, or the ones left out can't be found.

### What `host(cx)` gives a plugin

| Needs | API |
|---|---|
| nothing | `toast`, `hide`, `set_input` (chain tools: puts text in the launcher), `remember_input` (history), `settings` / `set_settings` (a JSON value the app keeps), `secret` / `set_secret` (Keychain-backed), `open_settings`, `open_window` (a window of the plugin's own; it hides with the launcher unless `WindowOptions::hide_with_launcher(false)`), `hide_window` / `show_window` (take one off screen and back), `confirm` (the system alert), `utc_offset_seconds`, `theme` |
| nothing | the **clipboard**, through GPUI's own `cx.read_from_clipboard()` / `cx.write_to_clipboard()`, by design open to every plugin |
| `Network` | `http`, `listen_http`, gRPC, `dns_resolvers` (see below) |
| `Commands` | `run` a listed program: no shell, empty environment, 60 s limit |
| nothing (temporary) | `open_url` (see below) |

## Temporary host APIs

Some of what plugins get from the app stands in for what embedded_gpui or WASI will give them directly.
Once it does, these are deprecated, then removed, and **plugins that use them must be updated**. Each is
kept apart in the code and marked, so `grep -rn "TEMPORARY(<name>)"` lists all of it.

| API | Today | Goes when | Plugins then |
|---|---|---|---|
| **The network** (`TEMPORARY(network)`) | `host(cx).http(request, cx)` with the `http` crate's types; `host(cx).listen_http(…)` for a sign-in's redirect on `127.0.0.1`; gRPC via tonic's generated clients (`network::grpc::channel`, the `grpc` feature). The app does HTTP/1.1, HTTP/2 and TLS natively, and macOS checks certificates. WASI's own sockets work too. | embedded_gpui links `wasi:http` (with WASI 0.3's async) | use ordinary HTTP and gRPC clients |
| **Opening a URL** (`TEMPORARY(open_url)`) | `host(cx).open_url(url, cx)` opens it in the app macOS has for it: a browser, the mail app for `mailto:`. Not `file:`. | embedded_gpui forwards GPUI's own `cx.open_url` from plugins | call `cx.open_url` |
| **Clipboard freshness** (`TEMPORARY(clipboard)`) | Nothing to call. Delight refreshes a plugin's copy of the clipboard early, so the first paste after copying elsewhere is the latest copy. | embedded_gpui delivers the change before the key | nothing changes |

Also expect changes while the protocol is below 1.0 (the current `0.0`): the host API, the action
model (`Shortcut`, styles) and the settings sections may still change between minor versions. The
release notes say when a plugin needs rebuilding.

## Building the app

```sh
cargo run -p delight-app                     # run the app
cd plugins && cargo build --release --target wasm32-wasip2   # the built-in plugins
```

The details (the WASI SDK the JSON and YAML built-ins need, signing, notarising, the release steps and
the AppKit workarounds waiting on GPUI) are in [`docs/development.md`](docs/development.md).

## License

MIT
