# Delight

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
