# Drawing the UI

A tool draws all of its own UI with [GPUI](https://www.gpui.rs), the same framework Delight is built
with. Delight gives a plugin no ready-made components: it gives it the app's **theme**, and the
plugin builds its own controls from GPUI's elements.

## Use the theme, always

The launcher is light or dark, as macOS is. A plugin that picks its own colours, or leaves GPUI's
defaults, is unreadable in one of the two: **the most common mistake is dark text on the dark
launcher.** Every view a plugin draws follows four rules:

1. **Every colour comes from the theme**: text, backgrounds, borders, icons. Never a colour of your
   own (`rgb(0x333333)`, `gpui::black()`, `gpui::white()`), which suits one appearance only.
2. **Leave text the theme's unless it sits on a colour.** Delight draws a plugin's views with the
   theme's text colour, size and font, so text you don't colour is `text`. Text on a background of
   your own takes that background's text colour (rule 3).
3. **Give each background the text colour made for it** (the table below). Light text on a light
   fill, or dark on a dark one, is as unreadable as no theme at all.
4. **Look at it in both.** Switch Settings → General → Appearance between Light and Dark with the
   tool open, and read every state: empty, loading, an error, a long result.

The theme is `delight_plugin_api::theme(cx)`. It is `None` until the app has answered, a moment
after the plugin starts: draw nothing until then rather than guess. When the user switches
appearance the theme changes, and Delight draws the plugin's views again.

```rust
use delight_plugin_api::theme;

fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let Some(t) = theme(cx).cloned() else { return div() };
    div()
        .flex()
        .gap(px(8.))
        .child(div().p(px(12.)).rounded(px(t.radius)).bg(Hsla::from(t.surface)).child("text on a surface"))
        .child(div().px(px(10.)).rounded(px(t.radius)).bg(Hsla::from(t.accent)).text_color(t.accent_text).child("Primary"))
        .child(div().text_color(t.text_muted).child("a detail"))
}
```

A theme colour is a `Color`; `.into()` (or `Hsla::from`) makes it GPUI's `Hsla`.

| Field | Is | Text on it |
|---|---|---|
| `background` | What the tool's view is drawn on (Delight draws it) | `text`, `text_muted`, `text_faint` |
| `window` | The launcher around it: its input, list and footer | |
| `text` | Body text | |
| `text_muted` | Captions, details, secondary columns | |
| `text_faint` | Hints and placeholders only | |
| `surface` | Raised areas: a result box, a field, a popover | `text`, `text_muted` |
| `card` | A card of rows, as Settings' groups | `text`, `text_muted` |
| `fill` | Controls: buttons, toggles | `text` |
| `hover` | A control or row under the mouse | `text` |
| `accent` | The selected or main control | `accent_text`, only |
| `selection` | Selected text's background | `text` |
| `focus_ring` | The ring around a focused control | |
| `border`, `separator` | Outlines; hairlines between rows | |
| `success`, `warning`, `error` | Status | Use it as the text or icon colour, on `background`, `surface` or `card`. Behind such text, the colour's tint: `t.tint(t.error)` (at `tint_opacity`) |
| `attention` | Something to do first (results gone stale) | |
| `syntax` | Highlighted code: `property`, `string`, `number`, `constant`, `comment`, `type_`, `keyword`, `punctuation` | (on `surface`) |
| `font`, `mono_font` | Fonts for interface text and for code | |
| `text_size`, `text_size_small`, `text_size_large`, `mono_size` | Sizes, in pixels | |
| `radius`, `radius_small` | Corner radii, in pixels | |
| `dark` | Which appearance it is | For the rare thing that must differ |

A colour that must stay fixed whatever the appearance is the one exception: a preview of an image
on white and on black, say.

## GPUI in a plugin

It is ordinary GPUI: a view implements `Render`, builds elements with `div()` and its style
methods, and redraws on `cx.notify()`. The plugin's GPUI draws into a surface Delight shows in the
tool pane, so a few things differ from an app:

- **The view fills the pane** under the tool's title: Delight wraps it in a full-size column. The
  pane is about 560 × 385 pt; don't depend on an exact size. The view reaches the footer: content
  that scrolls runs to the edge, and a view that ends with its last row leaves a little space after
  it.
- **Leave the background alone.** The launcher's window is the background, and a fill of your own
  over the whole pane looks like a box on it.
- **Import GPUI's prelude**, `use gpui::prelude::*;`. It has the traits most "no method named …"
  errors are about: `TaskExt` (`detach_and_log_err`), `StatefulInteractiveElement` (`on_click` on
  an element with an `.id(…)`), `AppContext`, `FluentBuilder` (`when`).
- **Overlays work**: tooltips and popovers a tool draws appear above the launcher.

### What GPUI can't do in a plugin

The plugin's GPUI runs in a sandbox, and some of its desktop calls do nothing there:

| GPUI's | In a plugin | Instead |
|---|---|---|
| `cx.open_url` | Does nothing | `host(cx).open_url` |
| `cx.open_window` | Refused: *plugin windows mirror host windows* | `host(cx).open_window` |
| `window.prompt` | Never answers | `host(cx).confirm` |
| File pickers (`prompt_for_paths`, `prompt_for_new_path`) | Fail | `host(cx).pick_folders`, `host(cx).save_file` ([Files](host-api.md#files)) |
| `reveal_path`, `open_with_system` | Do nothing | Nothing yet |
| `cx.hide`, `cx.quit`, `activate` | Do nothing | `host(cx).hide` |
| The credentials API | Reads nothing, can't write | `host(cx).secret` |
| `cx.write_to_clipboard` | Keeps the text only: no images or files | Text |
| `add_fonts`, the system's font list | Do nothing: Delight shapes a plugin's text | The theme's fonts, or an installed family by name; SVG icons rather than an icon font |
| `window.viewport_size()` | The whole launcher window, not the tool's pane | Layout (`size_full`, flex), or a `canvas`'s bounds |
| `window.appearance()` | Don't rely on it | The theme's `dark` |
| Gradients, inset shadows, transformed images | Solid colour, skipped, untransformed | Solid colours and paths |

## Controls

A control is an element with an `.id(…)`, a click handler and the theme's colours. The mode
buttons of the [tutorial](getting-started.md#8-draw-it) are one:

```rust
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
```

- **Keep the state in the view**, change it in the handler, then `cx.notify()`.
- **A control that can't be used now**: `.opacity(0.5)` and no handler.
- **Long text**: `.truncate()` keeps a label on one line, `.line_clamp(n)` to `n` lines. Cut text
  that can be very long (a pasted log line) to a few hundred characters before it is laid out.

## Text fields

GPUI has no text field, so a plugin has two ways to take text:

- **The launcher's input**, which most tools need: a keyword and what follows it
  (`weather london`). No field to draw.
- **A field of its own**, an element implementing GPUI's `EntityInputHandler`: Zed's
  `crates/gpui/examples/input.rs` is a whole one. It needs a `Window`, so make it the first time
  the view draws and keep it; bind its keys yourself with `cx.bind_keys` (the app's keymap doesn't
  reach a plugin); and **keep it out of anything that scrolls**: GPUI's branch panics (*prepaint has
  not been performed on …*) when a surface with a text field in it scrolls. Put the field above the
  tool's scrolling area.

## Scrolling

Give anything that scrolls a `ScrollHandle` kept in the view:

```rust
div()
    .id("results")
    .size_full()
    .overflow_y_scroll()
    .track_scroll(&self.scroll) // a ScrollHandle field
    .children(rows)
```

A tool that isn't shown is taken out of its plugin's window, and GPUI forgets the scroll offsets it
kept for it. With a handle of its own, the tool comes back where it was; without one it shows its
last picture for a moment, then jumps to the top.

## Focus and keys

- Make a control reachable from the keyboard with a focus handle made with
  `cx.focus_handle().tab_stop(true)`, on the element with `.track_focus(&handle)`, and handle its
  keys with `.on_key_down(…)`. → from the tool list enters the tool's first stop; ← on it goes back.
- Call `cx.stop_propagation()` only for the keys you handled: the others go back to the launcher,
  so the footer's keys work in the tool too.
- **Esc** hides the launcher. In a window of the plugin's own, Esc is the plugin's.

## Icons and images

**Icons** are SVGs drawn in one colour, which you give: the theme's.

```rust
svg().path("icons/copy.svg").size(px(14.)).text_color(t.text_muted)
```

GPUI reads the file from the plugin's assets: return an `AssetSource` of your own from
`Plugin::assets`.

```rust
struct Icons;

impl AssetSource for Icons {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "icons/copy.svg" => Some(Cow::Borrowed(include_bytes!("../assets/copy.svg"))),
            _ => None,
        })
    }

    fn list(&self, _path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(vec!["icons/copy.svg".into()])
    }
}

// in `impl Plugin`:
fn assets() -> Option<Box<dyn AssetSource>> {
    Some(Box::new(Icons))
}
```

**A picture in its own colours** (a logo, a preview) isn't an `svg()`: that draws a mask in the
text's colour. Render it yourself (with `resvg`, say) into a GPUI `RenderImage`, and draw it with
`img(ImageSource::Render(…))`. Images a plugin draws are kept for as long as it runs: make one per
picture, not one per frame or keystroke.

## Big content

- One frame may draw at most 100,000 primitives (boxes, borders, runs of text); past that the
  plugin stops. Show the first few hundred rows and say how many more there are, or use GPUI's
  `uniform_list`, which draws only the rows on screen.
- Keep work out of `render`: it runs on every frame. Parse, format and highlight when the input
  changes, and keep the result. Never start a task from `render`.
