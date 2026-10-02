# The plugin guide

Every tool in Delight is a plugin, the built-in ones too. This guide is for writing one: from an
empty folder to a plugin people install and keep up to date.

## A plugin in two minutes

A plugin is a Rust crate built as a WebAssembly component (`wasm32-wasip2`), installed as one
`.wasm` file. It has three parts:

1. **The plugin**: a type with `#[plugin(...)]`, which says what the plugin is: its id, name,
   icon, permissions and tips. The macro checks it when the crate compiles and stores it in the
   `.wasm`, where Delight reads it without running any code.
2. **Its operations**, the tools it offers: an enum with `#[derive(Operations)]`. On every change
   of the input, Delight calls the plugin's `detect`, which answers which of its tools fit and how
   well, from 0 to 1. The best one opens.
3. **Its tools**: GPUI views implementing `Tool`. A tool hears about the input, draws all of its UI
   itself, and offers footer actions on fixed keys (↵, ⌘↵, ⌥1–⌥9). **It draws with the app's theme
   colours only**, so it reads in light and in dark ([Use the theme,
   always](ui.md#use-the-theme-always)): dark text on the dark launcher is the most common mistake.

The plugin runs in its own sandbox with its own copy of GPUI, and reaches the app through
`host(cx)`: toasts, the clipboard, secrets, settings, windows, and the network or programs when
it has the permission.

## The pages

| | |
|---|---|
| [Getting started](getting-started.md) | Set up a crate, write a complete plugin step by step, build, install and test it |
| [How a plugin runs](concepts.md) | Detection and how sure to be, a tool's life, the one-second limit, the sandbox |
| [Plugins and tools](tools.md) | The manifest (`#[plugin]`, operations, icons, tips), the `Tool` trait, actions and keys |
| [Drawing the UI](ui.md) | The theme, and the rules for using it; GPUI in a plugin, controls, text fields, scrolling, icons |
| [Settings](settings.md) | A page of your own in Delight's Settings, drawn as the app's cards |
| [The host API](host-api.md) | Every call on `host(cx)`, saving data, the network, permissions and programs |
| [Publishing and versions](publishing.md) | Publishing at a URL, updates, which Delight runs a plugin, the temporary APIs |
| [Pitfalls](pitfalls.md) | The mistakes earlier plugins made, and how to avoid them |

## Examples

The built-in plugins in [`plugins/`](../../plugins) are plugins like any other, built against the
same API:

| Plugin | Shows |
|---|---|
| [Graphics](../../plugins/graphics) | The smallest: one tool, its picture rendered in the background |
| [Formats](../../plugins/formats) | Six tools in one plugin, the input parsed once in `detect` for all of them, results computed in tasks |
| [Network](../../plugins/network) | The `Network` permission: the Mac's DNS setup, WASI's sockets, a lookup debounced as you type |

Something unclear, missing or wrong? [Open an issue](https://github.com/brijsiyag/delight/issues).
