# Delight

A macOS launcher whose tools are plugins: WASM components that run their own
GPUI through embedded_gpui. `docs/plan.md` is the plan, `docs/behaviour.md`
what the app does.

## Development

- `cargo run -p delight-app` runs the app.
- The built-in tools are their own workspace, `plugins/`. Build them with
  `cargo build --release --target wasm32-wasip2` there; the app loads them from
  `plugins/target/wasm32-wasip2/release` when it runs outside Delight.app.
- Their C (tree-sitter, for syntax highlighting) needs the WASI SDK: its clang
  and C library for WebAssembly. Only building plugins needs it; the app and
  its users never do. Unpack the pinned SDK into `target/wasi-sdk`, where
  `plugins/.cargo/config.toml` points (or set `WASI_SDK_PATH` to your own), once:

  ```sh
  sdk=wasi-sdk-34.0-arm64-macos   # Intel Macs: wasi-sdk-34.0-x86_64-macos
  curl -fLO "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/$sdk.tar.gz"
  # arm64: 9c59398106b417f8f14913380fdf0097a8cc0ff4af9eb3ce0065a859e88d49e9
  # x86_64: 87d27fa8adc68dee59bfbf2e22a6d34ef717c34d6bf1d8af2a56fc929d9ce0eb
  shasum -a 256 "$sdk.tar.gz"
  mkdir -p target/wasi-sdk && tar -xzf "$sdk.tar.gz" -C target/wasi-sdk --strip-components 1 && rm "$sdk.tar.gz"
  ```

  It's about 600 MB; `cargo clean` deletes it with the rest of `target/`.
- Installed plugins are the `.wasm` files in
  `~/Library/Application Support/Delight/plugins`.

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
