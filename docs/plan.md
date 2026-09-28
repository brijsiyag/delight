# Delight 2: the rewrite plan

Delight is a macOS launcher: a hotkey opens one input line, what you type or
paste is offered to tools, and the chosen tool draws its own UI below the
input. Every tool is a plugin. This rewrite starts from an empty repository
and is designed around embedded_gpui from the first line, instead of
arriving at it after a dylib design.

The previous attempt (`~/Desktop/delight`, branch `feat/wasm-plugins`) is the
reference for *what* Delight does and how it looks; `docs/behaviour.md`
records that. Its code and folder layout are not reused.

## Decisions carried over

These were settled in the previous attempt (see its
`docs/wasm-rewrite-notes.md`) and still hold:

1. Plugins are embedded_gpui WASM components (`wasm32-wasip2`). A plugin runs
   its own GPUI; the app replays its display lists.
2. Built-in tools are plugins too; they take exactly the same path as
   installed ones.
3. Installing a plugin means adding one prebuilt `.wasm` file (Settings →
   Install…, or dropping it in the plugins folder). No CLI, no plugin
   manager, no SDK kit, no rebuild-on-update.
4. Permissions declared by the plugin gate everything outside its sandbox:
   `Network`, `Commands`, `Clipboard` (reading; copying needs none).
   Settings shows them; installing asks to confirm them.
5. No Delight HTTP API: the network is WASI's `wasi:http` and
   `wasi:sockets`, and plugins use `wstd`.
6. GPUI is not forked: it is used directly from Zed's repository by the
   app, embedded_gpui and every plugin.
7. embedded_gpui is the fork `brijsiyag/embedded_gpui`, used by path, never
   vendored; its changes stay small and upstreamable.

## What is different this time

- **The manifest is read without running the plugin.** It is written in
  the plugin's code (`#[plugin(...)]`, `#[derive(Operations)]`), and the
  macros put it (and the protocol version) in a custom section of the
  `.wasm` (`#[link_section]`; checked: it survives a stripped, LTO'd
  release build). The app reads it with `wasmparser` before compiling
  anything. So:
  - Install… shows the name, icon and permissions and asks, and no code from
    the file has run yet;
  - the protocol version is major.minor: additive changes bump the minor,
    and plugins built for an older minor keep working. A plugin built for
    another major, or a newer minor than the app's, is refused at load with
    a clear message instead of failing at its first call;
  - the WASI sandbox is configured once, at instantiation, from the granted
    permissions: no network switch flipped after start, and name lookups are
    allowed only for plugins with `Network`;
  - disabled plugins are listed with their names and icons but never
    started;
  - the id comes from the manifest, not the file name.
- **Permissions are capabilities.** embedded_gpui's authority model is
  reachability: a plugin can use only the objects it holds refs to. So a
  gated feature is an object the app hands over only when the permission is
  granted (commands, clipboard reading). There is no call-time permission
  check to forget. Changing a plugin's permissions restarts it.
- **Reactivity uses the object model.** The theme is a host object plugins
  observe, so a light/dark switch reaches every plugin on its own. A tool's
  footer actions are data it `cx.notify()`s about; the app observes the tool
  object, not its surface.
- **Built-ins are files.** Their `.wasm` files are packaged in the app
  (`Contents/Resources/plugins`, so in the DMG) and loaded by path, like
  installed ones: nothing is embedded in the binary, no build script builds
  plugins, and embedded_gpui needs no loading from bytes.
- **A headless test crate from the start** (a fixture plugin, a fake host
  root, no window), so plugin behaviour is tested without driving the GUI.
- **Fewer fork changes.** Network sockets and the data folder need none
  (upstream `PluginOptions::with_wasi` covers them); guest I/O is first
  tried in the plugin API (below).
- **GPUI stays on the branch embedded_gpui uses**
  (`gpui-embedded-in-gpui`, commit `7bc1c05`) instead of Zed's `main`, so
  the fork needs no port. Porting is left for later (step 2).

## Layout

```text
delight-umbrella/
├─ delight/                 this repository
│  ├─ crates/
│  │  ├─ manifest/          delight-manifest: the manifest and protocol version a .wasm carries
│  │  ├─ protocol/          delight-protocol: the contract (schemas + data)
│  │  ├─ plugin-api/        delight-plugin-api: what a plugin depends on
│  │  ├─ plugin-api-macros/ delight-plugin-api-macros: #[plugin] and #[derive(Operations)]
│  │  ├─ ui/                delight-ui: theme and shared components (native + wasm)
│  │  ├─ runtime/           delight-runtime: load, check and run plugins (native, no windows)
│  │  └─ app/               delight-app: the macOS app
│  ├─ plugins/              built-in plugins: their own workspace, wasm32-wasip2
│  ├─ tests/                the headless test crate; fixture/ is its plugin (own workspace)
│  ├─ xtask/                WASI SDK download, building built-ins, bundling, signing, version checks
│  └─ docs/
├─ embedded_gpui/           the fork, branch `delight`
└─ wstd/                    only if upstream won't take the reactor change (step 13)
```

wstd is plugin-side only: `delight-plugin-api` depends on it for `wasm32` targets,
so plugins get async HTTP and sockets over WASI. The app never links it; it
serves those WASI calls with wasmtime's `wasmtime-wasi` (sockets) and
`wasmtime-wasi-http` (HTTP), inside embedded_gpui.

Built-in plugins live in their own workspace because they only build for
`wasm32-wasip2`. A path dependency's `workspace = true` resolves in its own workspace, so the
crates they share (`manifest`, `protocol`, `plugin-api`, `ui`) stay members of the root one.
Every workspace (root, `plugins/`, the fork, third-party plugins) names GPUI
exactly as the fork does, `git = "https://github.com/zed-industries/zed.git",
branch = "gpui-embedded-in-gpui"` (plus `version = "=0.2.2"`), or Cargo links
two GPUIs: a `rev` for the same commit counts as a different source. Each
workspace's `Cargo.lock` pins the commit.

## How the pieces talk

```text
delight-app (native GPUI)
 ├─ launcher: input, matches, tool pane, footer
 ├─ settings window: general, plugins, plugin settings pages
 └─ delight-runtime, per plugin:
      manifest (read from the .wasm) → checks → PluginOptions
        (data folder, network if granted, limits)
      → embedded_gpui PluginHost → roots exchanged
            │  object protocol (delight-protocol schemas)
            ▼
plugin (.wasm, its own GPUI)
 └─ delight-plugin-api: the author's `Plugin` and tools behind the protocol,
    `host(cx)` facade, theme mirror, and (with `network`) wstd re-polling
```

The contract, in `delight-protocol` (names are provisional; each is settled
in its step):

- **Host root**, one per plugin, so no plugin id ever travels: this
  plugin's settings, secrets and encrypt/decrypt; remember an input; set the
  launcher input (`set_launcher_input`); toast;
  hide; copy text or a file; open a URL; open its settings page; add a
  font; host facts (UTC offset, app pid); the theme object; and the gated
  objects this plugin was granted (run commands, read the clipboard).
- **Plugin root**: `detect(input) -> [(operation, confidence)]`,
  `open_tool(operation, surface) -> Ref<Tool>`, `open_settings(surface)`.
  A plugin offers one or more operations (tools), listed in its manifest.
  Whether it has settings isn't declared: the app asks the running plugin,
  and the plugin API answers from whether the author wrote a settings view (step
  12).
- **Tool**, homed in the plugin: `on_input_changed(input)`,
  `list_actions()` (observed), `perform_action(action)`.
- **Data**: manifest (id, name, version, description, author, icon SVG,
  operations (each may have its own icon), permissions), input (text),
  detection, action (id, label, and a shortcut: a keystroke, or explicitly
  click-only), theme, command and its output.
- `PROTOCOL_VERSION`, written into every plugin by `#[plugin]`.

`docs/behaviour.md` has the full behaviour each step reproduces.

## Steps

Each step is one reviewable piece. After each: it builds, its tests pass, it
is reviewed, and only then committed. Steps marked **embedded_gpui** change
our embedded_gpui fork (nothing else is forked); each such change is
proposed and confirmed before it is written, and is its own commit on the
fork's `delight` branch.

1. **Project setup** (this step): workspace, toolchain, rules
   (`CLAUDE.md`), this plan, the behaviour record.
2. **GPUI: the branch embedded_gpui uses.** Stay on Zed's
   `gpui-embedded-in-gpui` branch (commit `7bc1c05`), as upstream
   embedded_gpui does, with its Rust toolchain (1.95.0). No change to GPUI
   or embedded_gpui. Porting to Zed's `main` waits until something needs it
   or the branch is deleted (its one change merged into `main` as #60574,
   so the commit is on no other branch). The port was tried and is small:
   Rust 1.98.1, and compile fixes only (`paint_image` bounds, atlas key by
   value, `PaddedBool32`, new `Platform`/`PlatformWindow` items).
3. **No embedded_gpui changes to start with.** Built-ins load from files,
   so loading from bytes isn't needed. The compile cache and keyboard focus
   into a tool wait (see Later).
4. **`delight-protocol`**: the three interfaces with what the launcher
   needs (detect and open a tool; on_input_changed, list_actions and
   perform_action; toast, copy text and hide), their data, the manifest's data and its checks,
   `PROTOCOL_VERSION`. Unit tests for (de)serialisation. Later steps add
   their parts of the contract (history, theme, settings, the gated
   objects with their permissions, `copy_file`); the only permission to
   start with is `Network`. Step 5 settles how the manifest is stored in
   the `.wasm`, with its source.
5. **`delight-plugin-api` + a fixture plugin**: the `Plugin` / tool traits
   authors implement, the manifest macros (entry point + manifest section;
   inert natively so a tool's unit tests run on the host), the `host(cx)`
   facade. A minimal plugin under the headless test crate
   (`tests/fixture`). The manifest is code, with no separate file:
   `#[plugin(id, name, icon, …)]` on the plugin type holds its properties,
   and `#[derive(Operations)]` on a fieldless enum its operations, one
   `#[operation(id, title, …)]` per variant with an explicit id (stored by
   the app, so a rename in code changes nothing). The plugin's `type
   Operation` links the two. The proc macros (`delight-plugin-api-macros`)
   read the icon files at compile time, check everything (a mistake is a
   compile error; the version is the crate's), and join it into one
   `delight-plugin` custom section: three JSON lines, the protocol
   version, the properties and the operations. The manifest and protocol
   version live in `delight-manifest`, which has no embedded_gpui, since
   what a proc macro depends on is built natively for every plugin.
6. **`delight-runtime`**: read and check a manifest from bytes (id,
   protocol version, permissions), build `PluginOptions`, start, exchange
   roots, plugin stopped/crashed reporting,
   detection across plugins and its ranking (a pure function). The headless
   test crate: fake host root, the fixture plugin, open a tool on an
   unattached surface, send it an input, read its actions.
7. **App shell**: single instance, tray icon and menu, global hotkey, the
   launcher window with its input; no tools yet.
8. **Launcher with tools**: load built-ins and the plugins folder, show
   matches, open a tool on a surface, footer actions and their keys, Esc
   and reopen behaviour.
9. **Input history**: ⌃R, ⌃N / ⌃P completions, history per tool.
10. **`delight-ui` + theme**: the theme on both sides (host object observed
    by plugins), the shared components the built-ins need.
11. **Built-in JSON, YAML and SVG plugins**: the WASI SDK xtask (tree-sitter
    is C), an xtask building `plugins/`, and the app loading the `.wasm`
    files from its built-in plugins folder (the build output during
    development).
12. **Settings window and permissions**: general settings, the plugins page
    (list, enable, permissions shown, Install… with confirmation, plugin
    settings pages on surfaces), the gated host capabilities (commands,
    clipboard reading), `add_font`, host facts.
13. **Network**: sockets through `with_wasi` for plugins with `Network`;
    **embedded_gpui**: link `wasi:http` with an outgoing sender whose TLS uses the
    macOS trust store (`rustls-platform-verifier`; bundled roots fail
    behind a company TLS proxy). In the plugin API (behind a `network` feature),
    wstd driven by GPUI: a wstd future checks its own pollable every time
    it is polled, so the plugin API re-polls pending network futures from a GPUI
    timer, and embedded_gpui needs no I/O driver. The one thing released
    wstd lacks is a public way to make its reactor current without
    `block_on` (about ten lines): propose it upstream first; fork wstd only
    if that is refused or slow.
14. **DNS tool**, then port the third-party plugins in
    `~/Desktop/delight-plugins` (the image plugin waits for pasted files,
    see Later).
15. **Secrets, updates, release**: encrypted plugin secrets (one Keychain
    master key), automatic updates, bundling (the built-in plugins go in
    `Contents/Resources/plugins`), signing, notarisation, the DMG, version
    checks.

## Later

Not needed to get the app working; each waits until it is.

- **Compile cache** (**embedded_gpui**): every start compiles each plugin
  (~250 ms, ~200 MB); wasmtime's own cache (`cache` feature,
  `PluginOptions::with_compile_cache(dir)`) brings it to ~10 ms.
- **Keyboard focus into a tool** (**embedded_gpui**: `Surface: Focusable`):
  a surface takes focus only when clicked. Until then tools work through
  the input and their footer actions.
- **Pasted files**: Finder files pasted into the input, shown as tags and
  passed to tools with the text, their contents only with an `InputFiles`
  permission. `Input` is a struct so they can join it as a minor protocol
  change. The third-party image plugin waits for this.
- **Zed's `main`**: see step 2.

## Watch out for

(Each cost time in the previous attempt; details in its notes.)

- Zed's repo has two packages named `gpui`: depend with `version = "=0.2.2"`.
- Crates GPUI links too (`resvg`, `regex`, `image`): pin the versions GPUI
  uses.
- tree-sitter is C: WASM builds need the WASI SDK's clang (`WASI_SDK_PATH`).
- Inside a plugin GPUI can't write the clipboard, open URLs, list or load
  system fonts, or tell the appearance; `chrono::Local` is UTC and there is
  no process id. The host root covers each of these.
- `#[interface]` makes one message type per method name at module level:
  method names must be unique across interfaces and not clash with types.
- Payload types need `Describe`; bytes cross as base64, not number arrays.
- A plugin turn has a 1 s budget: big parses go in tasks.
- Disk: each GPUI build is several GB.

## Open questions

- Whether stopping and reporting a crashed plugin needs a fork change (an
  event from `PluginHost`) or can be seen from failed calls. Decided in
  step 6.
- Network, step 13: WASI network (above) or a host HTTP API. The latter
  needs no wstd change but adds a Delight method per kind of network use.
  Also: wstd behind a plugin API `network` feature, or kept out of it. And
  the re-poll interval (short while a request is in flight, backing off for
  long waits such as a sign-in redirect).
- WASI 0.3: wstd's `main` is adding it, and there the host drives async and
  wstd has no reactor, so no change would be needed. Not usable yet;
  revisit if embedded_gpui moves to it.
