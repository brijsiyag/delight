# Contributing to Delight

Thanks for helping. This page is for working on Delight itself: the app, the plugin API, the UI kit
and the built-in plugins. To write a plugin of your own, the [plugin guide](docs/plugins/README.md)
is all you need.

## Reporting a problem

[Open an issue](https://github.com/brijsiyag/delight/issues) with what you did, what you expected
and what happened, Delight's version (Settings → General, at the bottom), and the log: *Open Logs*
in the menu bar. For a plugin, say which one and its version.

## Before you start

- **[`docs/plan.md`](docs/plan.md)** is the plan: the decisions, the layout, the order of work and
  what to watch out for. Read it first.
- **[`docs/behaviour.md`](docs/behaviour.md)** is what the app does, in detail: sizes, keys,
  limits, messages.
- **[`docs/development.md`](docs/development.md)** is how releases are made, the embedded_gpui
  fork, the temporary host APIs and the AppKit workarounds waiting on GPUI.

## Set up

- macOS, with Xcode and its Metal toolchain (GPUI compiles its shaders with it):
  `xcodebuild -downloadComponent MetalToolchain`.
- Rust through [rustup](https://rustup.rs). `rust-toolchain.toml` pins Rust 1.95.0 and the
  `wasm32-wasip2` target, and rustup fetches them.
- **Disk.** Each GPUI build takes several GB; the app's debug build, the built-ins and the tests'
  plugin together take tens of GB. Check `df -h ~` before a large build. Cargo's `incremental`
  folders are safe to delete when space runs out.
- The WASI SDK, for the built-ins' C (tree-sitter): `cargo xtask wasi-sdk` fetches the pinned one
  into `target/wasi-sdk`, once.

## Build and run

```sh
cargo run -p delight-app                                        # the app
cd plugins && cargo build --release --target wasm32-wasip2      # the built-in plugins
```

Run outside `Delight.app`, the app loads the built-ins from `plugins/target/wasm32-wasip2/release`,
so rebuild them to try a change in one. Installed plugins are in
`~/Library/Application Support/Delight/plugins`.

## Test

```sh
cargo test                       # the app's crates, and the headless plugin tests
cd plugins && cargo test         # the built-ins' logic, natively
```

`tests/` runs a fixture plugin (`tests/fixture`) in `delight-runtime` without a window: its
manifest, detection, tools, actions, the host API, the network and commands. It builds the fixture
itself. **Test plugin behaviour there, headless, never by sending keystrokes to the running app**:
they go into whatever you are typing in.

## Where things are

| Path | What |
|---|---|
| `crates/app` | The app: launcher, settings, install window, plugins' windows, history, updates, the menu bar |
| `crates/runtime` | Runs plugins (wasmtime through embedded_gpui), and the host side of what they call: HTTP, commands, DNS, opening URLs |
| `crates/protocol` | The objects the app and plugins exchange, and the light and dark themes |
| `crates/manifest` | What a `.wasm` says about itself, the protocol version, and the published files (`<id>.xml`) |
| `crates/plugin-api`, `crates/plugin-api-macros` | What plugins are written with: `#[plugin]`, the traits, `host` |
| `crates/ui` | The UI kit: icons, components, the text editor, drawn with the theme it reads; the app's and the built-ins' |
| `plugins/` | The built-in plugins (Formats, Network, Graphics), a workspace of their own (they build only for `wasm32-wasip2`) |
| `tests/` | The headless tests and their fixture plugin |
| `xtask/` | Building, bundling, signing and releasing (`cargo xtask help`) |
| `packaging/macos` | `Info.plist`, the icon, the entitlements |
| `docs/` | The documentation ([index](docs/README.md)) |

## How changes are made

- **Small steps.** Each change is one logically separate piece that can be reviewed alone. Finish
  it, check it builds and its tests pass, then hand it over.
- **The app is generic.** Tools draw all of their own UI; the app knows nothing about JSON or DNS.
  Keep the app's own UI minimal.
- **Prefer a popular crate** to code of our own, and never shell out to read system state: use a
  crate or an API.
- **Pin every dependency exactly** (`=x.y.z`, or a git `rev`), declared once in the root
  `[workspace.dependencies]`. GPUI is the exception: it is named by branch, exactly as embedded_gpui
  names it, and `Cargo.lock` pins the commit. Crates GPUI also links (`resvg`, `image`) are pinned
  at GPUI's versions.
- **A tool is self-contained**: its code, assets and tests in its folder. `delight-ui` holds only
  what more than one place uses.
- **Don't run `cargo fmt`** over the tree; match the formatting around your change.
- **Temporary APIs are marked.** Code that stands in for what embedded_gpui or WASI will provide
  lives apart, with `TEMPORARY(<name>)` on every line outside its folder, so
  `grep -rn "TEMPORARY(<name>)"` finds all of it (see `docs/development.md`).

### Changing the plugin API

What plugins see is versioned (`crates/manifest/src/version.rs` has the rules):

- **Minor** for what plugins built before survive: a new host method, a new field the app sends,
  a new field with a default that plugins send, a plugin method the app copes with older plugins
  lacking.
- **Major** for anything else that reaches built plugins: removing or renaming a method, changing a
  type, a new enum variant sent to plugins.
- The plugin API's crates share one version, in `crates/manifest/Cargo.toml`. Describe the new
  version in `PROTOCOL_VERSION`'s comment, rebuild the built-ins and the fixture, add a headless
  test, and update the [plugin guide](docs/plugins/README.md).

### Changing embedded_gpui

Delight uses the fork `github.com/brijsiyag/embedded_gpui`, branch `delight`, until upstream has
what it needs. Changes there are kept small and upstreamable, one commit each, and proposed upstream;
describe one in an issue before writing it. Code that relies on a change only the fork has is marked
`TEMPORARY(fork_<change>)`, so it is found and moved to upstream's API as soon as upstream releases
the change. `docs/development.md` ("The embedded_gpui fork") says how the branch is kept up to date.

## Commits and pull requests

- One logical change per commit, with a title that says what changed and a body of short bullets:
  what, why, and how it was tested.
- Update the docs in the same change: `docs/behaviour.md` for behaviour, the plugin guide for
  anything plugin authors see, `docs/plan.md` when a step is done.
- The pre-commit hook scans for secrets (TruffleHog). Never skip it.

## Releasing

`docs/development.md` ("Releasing") has the whole process: versions, the checks, building,
signing, notarising and publishing with `cargo xtask`.

## License

By contributing you agree that your contributions are licensed under the [MIT License](LICENSE).
