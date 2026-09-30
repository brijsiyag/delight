# Building, releasing and workarounds

The developer-facing half of the old README: how to build the app and the built-in plugins, how a release
is made, and the temporary host APIs and AppKit workarounds. The README is for people who use Delight
and write plugins for it.

A macOS launcher whose tools are plugins: WASM components that run their own
GPUI through embedded_gpui. `docs/plan.md` is the plan, `docs/behaviour.md`
what the app does, and `docs/plugin-settings.md` how to design a plugin's
settings.

## Development

- `cargo run -p delight-app` runs the app.
- The built-in tools are their own workspace, `plugins/`. Build them with
  `cargo build --release --target wasm32-wasip2` there; the app loads them from
  `plugins/target/wasm32-wasip2/release` when it runs outside Delight.app.
- Their C (tree-sitter, for syntax highlighting) needs the WASI SDK: its clang
  and C library for WebAssembly. Only building plugins that contain C needs it (the
  JSON and YAML built-ins do; a plugin in pure Rust never does), and the app and its
  users never do. Fetch the pinned SDK once with `cargo xtask wasi-sdk`: it downloads
  it (about 170 MB, 600 MB unpacked), checks its SHA-256 against the pin and unpacks it
  into `target/wasi-sdk`, where `plugins/.cargo/config.toml` points (or set
  `WASI_SDK_PATH` to your own). It does nothing if that version is already there;
  `--force` fetches it again, and `--to <folder>` unpacks it elsewhere. `cargo clean`
  deletes it with the rest of `target/`.
- Installed plugins are the `.wasm` files in
  `~/Library/Application Support/Delight/plugins`.

## Releasing

A release is one disk image, `dist/Delight-X.Y.Z.dmg`, plus the update feed
`dist/appcast.xml`. The disk image is both the first-install download and what Sparkle
updates from, so nothing else (no loose `.app`, no `.pkg`, no `.zip`) is published. The
tasks are in `xtask/` (`cargo xtask help`); who signs it, where it is published and what it
downloads are in `xtask/src/config.rs`.

### Once, on the Mac that builds releases

1. **The signing certificate.** A *Developer ID Application* certificate for team
   `S3L4RJ57GY` in the login keychain, with its private key. Check:
   `security find-identity -v -p codesigning` lists it. (No *Installer* certificate: there is
   no `.pkg`.)
2. **Notarisation credentials**, stored under the profile `delight-notary`. `notarytool`
   asks for an app-specific password from `account.apple.com`:

   ```sh
   xcrun notarytool store-credentials delight-notary --apple-id "APPLE_ID_EMAIL" --team-id S3L4RJ57GY
   ```

3. **The update-signing key.** Sparkle's private key is in the login keychain under the
   account `delight`, and its public half is `SUPublicEDKey` in `packaging/macos/Info.plist`.
   It is the same key as before, and must stay: a new one would stop installed copies
   updating. To move to another Mac, export it from the old one and import it on the new:

   ```sh
   dist/Sparkle-2.9.6/bin/generate_keys --account delight -x delight-sparkle.key   # old Mac
   dist/Sparkle-2.9.6/bin/generate_keys --account delight -f delight-sparkle.key   # new Mac
   ```

   (`cargo xtask sparkle` downloads Sparkle into `dist/` first.) Keep the file safe and
   delete it after.
4. **Rust and Xcode.** Both Mac targets, and Xcode's Metal compiler, which GPUI builds with:

   ```sh
   rustup target add aarch64-apple-darwin x86_64-apple-darwin
   xcodebuild -downloadComponent MetalToolchain
   ```

   The WebAssembly target and the WASI SDK the built-in plugins need are fetched by the
   tasks (`rust-toolchain.toml`, and `cargo xtask wasi-sdk`).

### Each release

1. **Set the version** in `Cargo.toml` (`workspace.package.version`) and the same in
   `plugins/Cargo.toml`. The plugin API's version (`crates/manifest/Cargo.toml`) is its own and
   changes only with the protocol. Commit.
2. **Check** the versions and the tag agree, before anything is built:

   ```sh
   cargo xtask check-versions vX.Y.Z
   ```

3. **Build the release:**

   ```sh
   cargo xtask release-macos vX.Y.Z
   ```

   It runs these stages, each of which can be run alone when something goes wrong:

   | Stage | Does | Leaves |
   |---|---|---|
   | `check-versions` | Checks the app, the built-in plugins and the tag agree | nothing |
   | `bundle-macos` | Builds the built-in plugins (`wasm32-wasip2`) and the app for both architectures, joins them (`lipo`), and assembles `Delight.app`: Sparkle in `Contents/Frameworks`, the plugins in `Contents/Resources/plugins`, `Info.plist` with the version and the build number (commits so far), the icon | `dist/Delight.app`, unsigned. `--native` builds for this Mac only, to try a bundle sooner |
   | `sign-macos` | Signs Sparkle's parts, then the app, with the Hardened Runtime and `packaging/macos/Delight.entitlements`, and verifies | the app, signed |
   | `package-macos` | Has Apple notarise the app and staples the ticket to it; puts it in a disk image with a shortcut to Applications; signs, notarises and staples the image; checks Gatekeeper accepts it; has Sparkle write the feed, signed with the update key | `dist/Delight-X.Y.Z.dmg`, `dist/appcast.xml` |

   Notarisation waits for Apple, usually a few minutes.
4. **Try it.** Open the disk image, drag Delight to Applications, start it, and check: the
   menu bar icon, the launcher, the built-in tools (JSON, YAML, SVG, DNS), and Settings →
   General → Updates. A plugin that won't start in a signed build is the first sign the
   entitlement is missing.
5. **Publish** (the tag is created with the release), with the two files the task prints:

   ```sh
   gh release create vX.Y.Z dist/Delight-X.Y.Z.dmg dist/appcast.xml --generate-notes
   ```

   Installed copies read `https://github.com/brijsiyag/delight/releases/latest/download/appcast.xml`
   once a day. That URL follows the *latest* release, so a draft or pre-release isn't seen.

### Why the entitlement

Plugins are WebAssembly, which wasmtime compiles to machine code as Delight starts them. The
Hardened Runtime, which notarisation requires, kills a process that makes memory executable
unless it has `com.apple.security.cs.allow-unsigned-executable-memory`; `allow-jit` alone is
not enough. This was tried on a signed test binary: no entitlement and `allow-jit` were
killed, this one ran. The old `disable-library-validation` entitlement was for plugins that
were dynamic libraries, and isn't needed (Sparkle is signed again by the team). There is no App
Sandbox: plugins with the `Commands` permission run system programs.

### Other teams and profiles

The tasks use only the team's certificates. To release from another deliberately, set
`DEVELOPER_TEAM_ID` and `DEVELOPER_ID_APPLICATION` (the certificate's name) together; set
`NOTARY_PROFILE` if the keychain profile has another name.

## Temporary host APIs

Some of what plugins get from the app stands in for what embedded_gpui or WASI
will give them directly; once they do, these are deprecated, then removed. To
make that easy, each is apart: a folder of its own in each crate, and a marker on
every line outside it, so `grep -rn "TEMPORARY(<name>)"` lists all of it. New
APIs of this kind follow the same rule.

- **The network**, `TEMPORARY(network)`. Plugins with the `Network` permission
  get the app's HTTP: requests with the `http` crate's types
  (`host(cx).http(…)`), HTTP callbacks on `127.0.0.1` (`host(cx).listen_http(…)`,
  for a sign-in's redirect), and gRPC with tonic's generated clients
  (`network::grpc::channel`, the plugin API's `grpc` feature). The app does the
  HTTP/1.1, HTTP/2 and TLS natively (hyper and rustls, on a small tokio runtime),
  so plugins don't block and TLS is checked by macOS. They also keep WASI's own
  sockets. It goes once embedded_gpui links `wasi:http` (with WASI 0.3's async,
  which drops the reactor problem of wstd #166), and plugins use standard HTTP
  and gRPC clients. Folders: `crates/protocol/src/network/`,
  `crates/runtime/src/network/`, `crates/plugin-api/src/network/`,
  `tests/network.rs`, `tests/fixture/src/network.rs`.
- **Opening a URL**, `TEMPORARY(open_url)`. `host(cx).open_url(…)` opens a URL
  with the app macOS has for it: a web page in the browser (a sign-in's),
  `mailto:` in the mail app, another app's own link; not `file:`. It goes once
  embedded_gpui forwards GPUI's own `cx.open_url` from plugins (its plugin
  platform drops it today). Folders: `crates/runtime/src/open_url/`,
  `crates/plugin-api/src/open_url/`, `tests/open_url.rs`.

- **Refreshing the clipboard early**, `TEMPORARY(clipboard)`. A paste in a plugin
  reads the plugin's own copy of the clipboard, which embedded_gpui refreshes when
  a ⌘ key reaches the plugin's surface (`host/surface.rs`, `key_down`). It only
  queues the change, and the key goes out first, so the first paste after copying
  something elsewhere read the old copy. Delight refreshes the copies ahead of that: when
  one of its windows gets the keyboard, and on clicks in its windows
  (`Plugin::refresh_clipboard`, `plugins::refresh_clipboards`). It goes once
  embedded_gpui delivers the change before the key; remove those and their calls and the
  test in `tests/plugins.rs`.

## Workarounds to remove

`crates/app/src/macos.rs` calls AppKit directly for what GPUI can't do
yet. Each one should go once GPUI offers it; checked against the GPUI
Delight uses (Zed's `gpui-multi-root-embedded-rebased` branch, commit
`8c88a5c`): it has none of them yet.

| Workaround | Remove when GPUI can |
|---|---|
| `set_accessory_app`: no Dock icon or app menu | start an app as a menu-bar-only (`Accessory`) app; it always sets `Regular` at launch |
| `style_floating_panel`: the borderless restyle, and giving the keyboard back to GPUI's view after it | open a borderless window on macOS; with `titlebar: None` it still makes a titled one |
| `style_floating_panel`: the Liquid Glass or blur backdrop | draw a window background with Liquid Glass, or a blur shaped to the window's own corners (`Blurred` keeps macOS's corner shape) |
| `style_floating_panel`: `setHidesOnDeactivate(false)` | keep a pop-up window showing when the app deactivates (it's a panel, and panels hide) |
| `style_floating_panel`: `setHasShadow(true)` | give a window a system shadow |
| `set_corner_radius`, `rounded_mask` | round a window's corners |
| `resize_keep_top` | resize a window keeping its top edge, animated (`resize` keeps the bottom edge) |
| `present`, `hide` | tell which app was in front and give it back the keyboard, and hide one window (`cx.hide()` hides the whole app) |
| `is_window_visible` | tell whether a window is on screen |

`NativeWindow` exists because AppKit calls back into GPUI while a window
changes, and GPUI drops those callbacks during its own updates ("RefCell
already borrowed"); the calls above run from spawned tasks for that reason.
It goes with them.
