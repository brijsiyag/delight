# Delight

A macOS launcher whose tools are plugins: WASM components that run their own
GPUI through embedded_gpui. `docs/plan.md` is the plan, `docs/behaviour.md`
what the app does.

## Workarounds to remove

`crates/app/src/macos.rs` calls AppKit directly for what GPUI can't do
yet. Each one should go once GPUI offers it; checked against the GPUI
Delight uses (Zed's `gpui-embedded-in-gpui` branch, commit `7bc1c05`).

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
