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
   `Network`, `Commands`, each with the plugin's reason for it. Settings
   shows them; installing asks to confirm them. The clipboard needs none (on
   2026-09-29 kept as it is, reading included, over paste-only reading: that
   would change embedded_gpui, or a best-effort wrapper in Delight):
   every plugin reads and writes it (⌘V pastes in its text fields).
5. The network, for now, is the app's (step 13): HTTP, HTTP callbacks and
   gRPC done natively and handed to plugins with `Network`, besides WASI's
   own sockets. It goes once embedded_gpui links `wasi:http`, and plugins use
   standard clients (README, "Temporary host APIs").
6. GPUI is not forked: it is used directly from Zed's repository by the
   app, embedded_gpui and every plugin.
7. embedded_gpui is used from upstream (`zed-industries/embedded_gpui`) at a
   commit (`rev`), never vendored, so the app and every plugin get exactly the
   same one. Changes are made in the fork `brijsiyag/embedded_gpui`, stay
   small and upstreamable, and are proposed upstream; Delight moves to the
   fork's commit only for a change it can't wait for. Since 2026-09-30 it
   uses the fork's `delight` branch (upstream's `surfaces-as-roots` plus
   hidden surfaces, since tools of one plugin took each other's input, and a
   compile cache), until upstream has both (`docs/development.md`, "The
   embedded_gpui fork").

## What is different this time

- **The manifest is read without running the plugin.** It is written in
  the plugin's code (`#[plugin(...)]`, `#[derive(Operations)]`), and the
  macros put it (and the protocol version) in a custom section of the
  `.wasm` (`#[link_section]`; checked: it survives a stripped, LTO'd
  release build). The app reads it with `wasmparser` before compiling
  anything. So:
  - Install… shows the name, icon and permissions and asks, and no code from
    the file has run yet;
  - the protocol version is the plugin API's version (the crates plugins
    build against: manifest, protocol, plugin-api and its macros, released
    together, apart from the app's version), major.minor: additive changes
    bump the minor, and plugins built for an older minor keep working. A
    plugin built for another major, or a newer minor than the app's, is
    refused at load with a clear message instead of failing at its first
    call;
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
  plugins, and embedded_gpui needs no loading from bytes. Every plugin file,
  built-in or installed, is named by its id, `<id>.wasm`, so a plugin is found
  by its id and each starts on its own: a built-in's id is its crate's library
  name, which Cargo names the file by (`delight_formats`; `delight.json` before
  2026-10-01), and installing names the file. A file named otherwise doesn't
  load, and says why.
- **A headless test crate from the start** (a fixture plugin, a fake host
  root, no window), so plugin behaviour is tested without driving the GUI.
- **Fewer fork changes.** Network sockets and the data folder need none
  (upstream `PluginOptions::with_wasi` covers them); guest I/O is first
  tried in the plugin API (below).
- **GPUI is the one embedded_gpui names** instead of Zed's `main`, so
  nothing needs a port: Zed's `gpui-multi-root-embedded-rebased` (commit
  `8c88a5c`) since 2026-09-29, `gpui-embedded-in-gpui` (`7bc1c05`) before
  (step 2; Later, back to embedded_gpui's `main`).

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
│  ├─ xtask/                building and distributing the app: bundling, signing, version checks (step 15)
│  └─ docs/
├─ embedded_gpui/           the fork, branch `delight`: where embedded_gpui changes are made
└─ wstd/                    only if upstream won't take the reactor change (step 13)
```

wstd is plugin-side only: `delight-plugin-api` depends on it for `wasm32` targets,
so plugins get async HTTP and sockets over WASI. The app never links it; it
serves those WASI calls with wasmtime's `wasmtime-wasi` (sockets) and
`wasmtime-wasi-http` (HTTP), inside embedded_gpui.

Built-in plugins live in their own workspace because they only build for
`wasm32-wasip2`. A path dependency's `workspace = true` resolves in its own workspace, so the
crates they share (`manifest`, `protocol`, `plugin-api`, `ui`) stay members of the root one.
Every workspace (root, `plugins/`, embedded_gpui, third-party plugins) names GPUI
exactly as embedded_gpui does, `git = "https://github.com/zed-industries/zed.git",
branch = "gpui-multi-root-embedded-rebased"` (plus `version = "=0.2.2"`), or Cargo links
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
  font; host facts (the UTC offset); the theme object; and the gated
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
  operations (each may have its own icon), permissions, up to 5 tips), input (text),
  detection, action (id, label, and a shortcut: ↵, ⌘↵, ⌥1 to ⌥9, or explicitly
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
   value, `PaddedBool32`, new `Platform`/`PlatformWindow` items). On
   2026-09-29 Delight moved with embedded_gpui's `surfaces-as-roots` to the
   GPUI it names (see Later): one compile fix in the app (`Window::blur`
   takes `cx`).
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
   roots, plugin stopped/crashed reporting, detection across plugins and
   its ranking (a pure function). The headless test crate: fake host root,
   the fixture plugin, open a tool on an unattached surface, send it an
   input, read its actions. A stop needs no fork change: embedded_gpui
   fails the calls in flight when a plugin traps, so the first failed call
   marks it stopped and it isn't called again; calls also time out, since
   one made just after the stop is never answered.
7. **App shell**: single instance, tray icon and menu, global hotkey, the
   launcher window with its input; no tools yet. In two parts: the shell
   (single-instance lock, no Dock icon, the menu bar icon with Open and
   Quit, ⌘⇧Space toggling the bar, Esc and focus loss hiding it); then the
   launcher and `delight-ui`, ported from the previous attempt: the UI kit
   with a small theme, the input (its text editor), the bar growing into
   the panel with its empty states, the compiled-in keymap, and the macOS
   finish (the borderless restyle, backdrop and corners, and remembering
   and re-activating the previous app).
8. **Launcher with tools**: load built-ins and the plugins folder, show
   matches, open a tool on a surface, footer actions and their keys, Esc
   and reopen behaviour.
9. **Input history**: ⌃R, ⌃N / ⌃P completions, history per tool.
10. **Theme for plugins**: `HostApi::current_theme` (protocol 1.2), the
    app's theme as data; each plugin's host object notifies when it
    changes, so the plugin API keeps a copy (`theme(cx)`) and redraws.
11. **Built-in JSON, YAML and SVG plugins**, in two parts. First the
    `plugins/` workspace and the app loading the `.wasm` files from its
    built-in plugins folder (the build output during development);
    `delight-ui` in plugins (its wasm build, its theme from `theme(cx)`,
    its icons through `Plugin::assets`); and the SVG tool, which needs no C
    (copying as PNG waits for `copy_file`). Then, after step 12 (settings
    and installing plugins come first): the WASI SDK (tree-sitter is C:
    its clang and wasi-libc build it for wasm32-wasip2; developers unpack
    the pinned SDK into `target/wasi-sdk`, where `plugins/.cargo/config.toml`
    points `WASI_SDK_PATH`, and `cargo xtask wasi-sdk` fetches it and checks its SHA-256),
    syntax highlighting, and the JSON and YAML tools.
12. **Settings window and permissions**, a new design rather than the
    previous attempt's two tabs: an 800×580 window with a sidebar, like
    System Settings (General, each plugin as its own entry, plugins that
    don't load, Install Plugin…). In three parts: the settings file and the
    General page (the shortcut, hiding, pasting on open, appearance, the
    input history, open at login), opened from the tray, ⌘, and the
    footer's ⚙ (gone since 2026-10-01). Then the plugin pages: their tools, each with its own switch
    (turned-off plugins and tools are stored apart, and a tool runs only
    while both are on), permissions, on and off, delete, show in Finder,
    plugins that don't load, and installing with a sheet that lists the
    permissions and the tools. Then each plugin's own settings, as sections:
    the plugin names them (id, title, height, footer) and draws the rows in
    each on a surface, while the app draws the title and the card, so they look
    like the app's own; a plugin says when they changed (`settings_changed`); each permission with the plugin's reason for it, shown under
    what the permission allows; and `Granted`, the objects the manifest's
    permissions grant, which the app's root object for the plugin hands out
    when asked (so a plugin without the permission gets none). It has none
    yet: the gated capabilities, `add_font` and the host facts come with the
    plugins that need them (step 14).
13. **Network**, temporary until embedded_gpui links `wasi:http` (README,
    "Temporary host APIs"; every piece in a `network/` folder or marked
    `TEMPORARY(network)`). Plugins with `Network` keep WASI's sockets
    (`with_wasi`), and get `HostApi::http`: requests streamed both ways
    through an exchange object and a plugin-homed receiver (the app waits for
    each piece to be taken: back-pressure), over hyper on a small tokio
    runtime, HTTP/1.1 and HTTP/2, TLS checked by macOS
    (`rustls-platform-verifier`); callback listeners on `127.0.0.1`, each
    request answered by the plugin's own responder; in the plugin API, the
    `http` crate's types (`host(cx).http`, `host(cx).listen_http`) and gRPC
    through tonic's generated clients (`network::grpc::channel`, the `grpc`
    feature). Headless tests against local HTTP and HTTP/2 servers.
14. **DNS tool**, then port the third-party plugins in
    `~/Desktop/delight-plugins` (the image plugin waits for pasted files,
    see Later). Each brings the host capabilities it needs; `open_url`, for
    a sign-in, is in already (temporarily: README, "Temporary host APIs"). The
    DNS built-in (`plugins/network`, since 2026-10-02 the Network plugin's tool) runs no programs: it asks the Mac's resolvers
    itself over WASI's UDP sockets (`hickory-proto` for the messages;
    `Network`), and the app tells it which they are (`DnsApi`, in `dns/`
    folders: which server answers which domain, VPNs' included, read from
    macOS's configuration store with `system-configuration` and from
    `/etc/resolver/` with `resolv-conf`, no program run); not temporary, as
    WASI has nothing for a host's DNS setup. Google Calendar and Logs bring the UTC offset as a host fact,
    and Lucide `add_font` (its icon font: the app shapes plugins' text, so a
    font a plugin loads itself isn't seen). The `Commands` permission
    (done, its own commit) is an object in `Granted` that runs only the
    programs its manifest lists (shown when installing), each an absolute
    path directly in `/bin`, `/sbin`, `/usr/bin` or `/usr/sbin`, with any
    arguments (not checked: no shell, so `|` and `;` are just text), a
    cleared environment, the plugin's data folder as the working folder,
    and a time limit. It is generic, for any plugin, so Process and Port
    kill use it rather than an API of their own; passing the right PID
    is the user's business (nothing stops Delight's own). Logs takes an API key on its settings page: ⌘V pastes
    it into the field, so its "Paste from clipboard" button can go. Done in
    `~/Desktop/delight-umbrella/delight-plugins`: Process, Port kill (new: `port <n>`), Logs and
    Google Calendar, with `secret` / `set_secret` (step 15's store, early: one
    Keychain master key, AES-256-GCM), `utc_offset_seconds` and
    `show_settings` as host facts, and the text editor's keys bound in every plugin
    (`delight_ui::key_bindings`). Logs and Google Calendar use the Network
    permission's HTTP; Valmo's self-signed cluster is a decision still open.
15. **Secrets, updates, release**: encrypted plugin secrets (one Keychain
    master key; done), automatic updates, bundling (the built-in plugins go in
    `Contents/Resources/plugins`), signing, notarisation, the DMG, version
    checks.
    - Done: `cargo xtask wasi-sdk` (the pinned SDK, SHA-256 checked); and the app's
      side of Sparkle (`updater.rs`): loaded from `Contents/Frameworks/Sparkle.framework`
      when the app is a `.app` that has it, daily checks, "Check for Updates…" in the
      menu bar menu and a Check Now row in General, the feed
      (`github.com/brijsiyag/delight/releases/latest/download/appcast.xml`) given by the
      delegate. Its Objective-C glue is tested; Sparkle itself is not yet run, as that
      needs the pieces below.
    - Done, written for this architecture (README, "Releasing"): `cargo xtask check-versions`,
      `bundle-macos`, `sign-macos`, `package-macos` and `release-macos`: the `.app`
      (Sparkle, the built-in plugins in `Contents/Resources/plugins`, `Info.plist` from
      `packaging/macos/`), signing, and one disk image that is both the first install and
      what Sparkle updates from, with the feed. Not run yet: it needs the build Mac's
      certificate, notary profile and update key. The EdDSA key pair already exists and must
      be reused: a new key would stop existing installs from updating. The signed app needs
      `allow-unsigned-executable-memory` (wasmtime's compiled plugins; tested on a signed
      test binary).
16. **Plugin updates.** Plugins are published at a location, such as a GitHub release's download
    address: for each plugin `<id>.wasm` and `<id>.xml`, a manifest with its id, version, name,
    description and the file's SHA-256 (`delight_manifest::Release`), and beside them a list of
    every plugin there in the same form (`PluginList`, under any name; the publish task names it
    `plugins.xml`). Each plugin is versioned on its own.
    The plugin names only its location (`#[plugin(update = "…")]`, optional). The app reads each
    installed plugin's `<id>.xml` at launch and once a day, and downloads `<id>.wasm` only when the
    version is newer; built-ins update with the app. A download is checked before anything runs:
    its SHA-256, its manifest (read without running it), the id and version the `.xml` said, newer
    than the installed one. An update that asks for nothing new (the same permissions, the same
    programs) replaces the file and restarts that plugin alone, keeping its data, secrets and
    settings; one that asks for more goes through the install window, like a new plugin. Each
    plugin's page has "Update automatically" (on by default, stored per plugin): it applies the first
    kind on its own, never while the plugin's tool is shown or one of its windows is open; the second
    kind always waits. The page also shows an update and installs it, or else what the last look
    found, with Check Now: everything about updates is per plugin, nothing is in General.
    Installing: Settings' "Install Plugin" is a small split button; its main part picks `.wasm`
    files (the macOS file picker), and its arrow opens a menu, "From a File…" and "From a Link…".
    From a link, the install window takes a link to an XML file, a list or one plugin's manifest,
    and tells which from the file's root element, `<plugins>` or `<plugin>`, never from its name
    (`Link`, `read_published`); it looks at it as soon as it is pasted, lists the plugins there with
    checkboxes (name, description, and "Update" or "Installed", which can't be picked), downloads
    and checks the picked ones with a progress bar under each, then Install shows each like a
    picked file. Publishing: Delight defines only the files at a location (README, "Publishing
    updates"); making them is the plugins' repository's. `delight-plugins` has `cargo xtask
    publish`, which writes every plugin's files and a list for a GitHub release (`--out`), or sends
    the new and newer ones to the location each names (`--upload`, HTTP PUT), and refuses a plugin
    whose file changed while its version stayed. Meesho's plugins
    are published as GitHub releases (`https://github.com/Meesho/delight-plugins/releases/latest/download`);
    `delight-plugins` names that location once it builds against a Delight release that has
    `update`. Not yet: signing releases (whoever can write a location can replace its plugins; the
    plugin's key would go in its manifest, as Sparkle's does for the app), putting back the previous
    file when an update doesn't start, and a mark in Settings' sidebar.

17. **The built-ins as three plugins** (2026-10-02): Formats (`plugins/formats`: JSON, YAML ⇄
    JSON, and new, Base64, .env ⇄ JSON, JWT read and verified, JSON signed as a JWT), Network
    (`plugins/network`: DNS lookup) and Graphics (`plugins/graphics`: SVG Preview), each a home
    for the tools of its kind, each tool with its own icon (Lucide glyphs). Results have no caption
    (the tabs or the title say what they are); DNS shows its records in one table. A tool that takes
    text (the .env prefix, a JWT's secret) has a live field above its scrolling pane
    (`formats/src/field.rs`), though GPUI's branch has panicked when text changed next to a field
    (see "Watch out for"): to watch. JWT signatures are HMAC (`hmac`, `sha2`), what a secret makes.
    `docs/behaviour.md`, "Built-in tools", has the rest.

## Later

Not needed to get the app working; each waits until it is.

- **Compile cache** (**embedded_gpui**, done 2026-09-30, in the fork): every start compiled each
  plugin (~250 ms, ~200 MB each). Delight can't cache it itself: `PluginInstance::new`
  (`host.rs`) makes its own wasmtime `Config` and `Engine`, and wasmtime's cache is off unless
  `Config::cache` is called. The change: a `CompileCache` the app makes once for a folder
  and hands to each plugin (`PluginOptions::with_compile_cache`); it is wasmtime's own cache
  (keyed by the component's contents and the engine's settings, pruned by one worker), with
  wasmtime's `cache` feature; Delight makes it in `~/Library/Caches/Delight/compiled`
  (`plugins/loading.rs`). A second commit on
  the fork's `delight` branch (`docs/development.md`, "The embedded_gpui fork"); proposed
  upstream.
- **Back to embedded_gpui's `main`**: since 2026-09-29 Delight uses its
  `surfaces-as-roots` branch (`ceb0df8`, not yet merged) and the GPUI it
  names, Zed's `gpui-multi-root-embedded-rebased` (`8c88a5c`: Zed PR #63800,
  a view tree, plus `Window::attach_root`). #63800 was closed unmerged on
  2026-09-25, so both branches may be reworked or deleted: move `rev` to
  embedded_gpui's `main` once the branch is merged there, and name GPUI as
  it does then. What Delight took from it (plugin API 2.0): plugins use
  GPUI's own clipboard calls, through embedded_gpui's clipboard object the
  app hands out (`HostApi::clipboard`: reading and writing, so ⌘V pastes in
  a plugin's text field), instead of `HostApi::copy_text`; `open_view` takes a finished
  view, so `Plugin::open_tool` and `settings_page` have no window; and a
  surface is a tab stop that passes focus to the tool's first control and
  back the keys the tool leaves alone, so → (or Tab) in the tool list moves
  into the tool and ← on its first control back. It also
  brings overlays (a plugin's tooltips and popovers, drawn above the app),
  IME and dead keys in plugins, and a stopped plugin's surface saying why.
- **Pasted files**: Finder files pasted into the input, shown as tags and
  passed to tools with the text, their contents only with an `InputFiles`
  permission. `Input` is a struct so they can join it as a minor protocol
  change. The third-party image plugin waits for this.
- **Calls to a stopped plugin fail at once** (done 2026-09-30, in Delight): when a plugin
  stops (a trap: a turn over its budget, out of memory, a panic, too much drawn),
  embedded_gpui fails the calls in flight, but a call made after the stop was sent to a
  worker that was gone and never answered, so the next one waited `CALL_TIMEOUT` (3 s).
  `PluginHost` tells whoever watches it when it stops, so Delight's `Plugin` watches its
  host and is marked stopped at once; every call site already checks that before calling.
  Chosen over a change in embedded_gpui (every later call failing there), which would have
  been a second commit to carry in the fork for nothing Delight needs; it could still be
  proposed upstream. `CALL_TIMEOUT` stays, for a plugin that runs but never answers.
- **Scroll position of a hidden tool** (done for the built-ins 2026-10-01; `delight-plugins` to
  do): since 2026-09-30 a tool that isn't selected is
  hidden (`Surface::set_hidden`, `docs/development.md`, "The embedded_gpui fork"): its view is
  taken out of its plugin's window and keeps its own state. What GPUI keeps for it instead, in
  the window's element state, is dropped with it: above all the offset of a scroll area the
  tool doesn't track itself (`.id(…).overflow_y_scroll()` with no `ScrollHandle`). So coming
  back to such a tool showed its last picture, scrolled, for a moment, then the top. The app
  can't help: it never sees a scroll offset (the display list has it baked into the positions),
  and the view isn't recreated, only re-placed. Two fixes:
  each plugin keeps its own scroll position (a `ScrollHandle` in the view, `track_scroll`: a
  small change per plugin, done in Delight's plugins (JSON, YAML, DNS), to do in
  `delight-plugins`); or GPUI keeps an attached root's node while it is detached, which is a
  change to Zed's branch (proposed upstream, not forked).
- **Zed's `main`**: see step 2.

## Watch out for

(Each cost time in the previous attempt; details in its notes.)

- A `TextEditor` on a page that scrolls inside a plugin, next to text rows, panics GPUI
  ("prepaint has not been performed on …", text.rs) when scrolled; not found in GPUI
  yet. Plugin settings avoid it (the app scrolls the page; a section's surface doesn't), and the
  Formats tools' fields sit above their scrolling panes. PagerDuty's settings also panicked when a
  text row changed next to a field; the Formats fields update their results live (since
  2026-10-02), so watch for it there.

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

- WASI 0.3: wstd's `main` is adding it, and there the host drives async and
  wstd has no reactor, so no change would be needed. Not usable yet;
  revisit if embedded_gpui moves to it.
