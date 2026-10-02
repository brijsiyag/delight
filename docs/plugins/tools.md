# Plugins and tools

What a plugin declares about itself, and what each of its tools implements.

## The manifest

What a plugin is, declared in code: `#[plugin(...)]` on the plugin's type and
`#[derive(Operations)]` on the enum of its tools. Both are checked when the crate compiles (a
mistake is a compile error at its line) and stored in the `.wasm`, where Delight reads them
without running the plugin.

### `#[plugin(...)]`

```rust
#[plugin(
    id = "com.example.weather",
    name = "Weather",
    description = "The forecast for any city.",
    author = "Example Inc.",
    icon = "assets/icon.svg",
    tags = ["weather", "forecast"],
    permissions = [Network("Reads forecasts from api.open-meteo.com")],
    tips = ["weather <city> shows its forecast"],
    update = "https://github.com/example/delight-plugins/releases/latest/download",
)]
struct Weather;
```

| Property | Required | What it is |
|---|---|---|
| `id` | yes | The plugin's identity. It names its files, folders, settings and secrets in Delight, so it never changes. 1–128 letters, digits, `.`, `_` or `-`, not starting with `.`. Reverse-DNS by convention |
| `name` | yes | Shown in Settings, when installing, and beside tools whose title differs |
| `icon` | yes | A path from the crate's `Cargo.toml` to an SVG ([Icons](#icons)) |
| `description` | | One sentence: what the plugin does |
| `author` | | Who made it, shown when installing |
| `tags` | | Words about the plugin. Stored; Delight doesn't use them yet |
| `permissions` | | What it may do outside its sandbox, each with why ([Permissions](host-api.md#permissions)) |
| `tips` | | Up to 5 hints for the launcher's empty input ([Tips](#tips)) |
| `update` | | Where it is published, so Delight can update it ([Publishing](publishing.md)) |

The **version** is the crate's `version`. Delight updates only to a newer one, so bump it for
every release.

### `#[derive(Operations)]`

A plugin's tools, called *operations*: a fieldless enum, one variant per tool.

```rust
#[derive(Operations)]
enum WeatherOperation {
    #[operation(id = "forecast", title = "Forecast", description = "The next seven days")]
    Forecast,
    #[operation(id = "radar", title = "Rain Radar", icon = "assets/radar.svg")]
    Radar,
}
```

| Property | Required | What it is |
|---|---|---|
| `id` | yes | Unique in the plugin, and stored by Delight (the input history, the tool that was picked), so it never changes. Renaming the variant changes nothing |
| `title` | yes | The tool's name in the list and above its view |
| `description` | | What it does, beside the tool in Settings |
| `icon` | | The tool's own icon; without one it shows the plugin's |
| `tags` | | As the plugin's |

### Icons

- **Square, full colour, its own background.** Delight doesn't tint it: it is drawn as it is on
  light and dark. A rounded square of one colour with a white glyph reads well at 20 px, the size in
  the tool list.
- **No text.** Icons are drawn without fonts, so a `<text>` element doesn't show: turn letters into
  outlines (paths).
- **One per tool where it helps**, to tell a plugin's tools apart in the list.
- **The glyph's licence.** A glyph from an icon set ([Lucide](https://lucide.dev), say) comes with
  its licence: keep it beside the icons in `assets/`.

### Tips

Shown now and then as the placeholder of the launcher's empty input, while the plugin has a tool
on. Say what to type and what it gives (`weather <city> shows its forecast`, *Paste JSON to format
it*), not keys: the footer shows those. One or two; at most 5, of 60 characters each.

### What is checked

The build fails, at the line that is wrong, when:

- the id is empty, longer than 128 characters, starts with `.` or has other characters;
- the name is blank, or an icon file can't be read;
- a permission has no reason, a blank one or one over 100 characters, or is asked for twice;
- `Commands` lists no programs, more than 20, one twice, or one that isn't an absolute path directly
  in `/bin`, `/sbin`, `/usr/bin` or `/usr/sbin`;
- there are more than 5 tips, or one is blank or over 60 characters;
- `update` isn't an `http://` or `https://` URL with a host, has spaces, or is over 2,048 characters;
- there is no operation, two have the same id, or one has no id or title.

## Tools

A tool is a GPUI view that implements `Tool`: Delight shows it in the launcher's pane, tells it the
input, and shows its actions in the footer.

```rust
pub trait Tool: Render {
    type Action: Actions;

    fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>);
    fn list_actions(&self, cx: &App) -> Vec<Action<Self::Action>>;
    fn perform_action(&mut self, action: Self::Action, cx: &mut Context<Self>);

    fn on_focus_lost(&mut self, cx: &mut Context<Self>) {}
    fn on_shown(&mut self, cx: &mut Context<Self>) {}
    fn on_hidden(&mut self, cx: &mut Context<Self>) {}
}
```

| Method | Called when | Do |
|---|---|---|
| `on_input_changed` | The tool opens, and when it is shown with an input it hasn't had | Keep what you need of the input, start the work, `cx.notify()` |
| `list_actions` | After the tool notifies | Return the footer's actions now, in order |
| `perform_action` | An action's key or button | Do it |
| `on_shown` | It was picked, or the launcher came back with it picked | Refresh what goes stale (a list from a server); start polling |
| `on_hidden` | Another tool was picked, or the launcher hid | Stop polling and animations |
| `on_focus_lost` | A click elsewhere in the launcher (its input, list, footer) | Close a menu the tool holds open |

`Plugin::open_tool` makes the tool, once, the first time it is picked; Delight keeps it with its
state while the plugin runs ([A tool's life](concepts.md#a-tools-life)).

### The input

```rust
pub struct Input {
    pub text: String,
}
```

The launcher's text as typed or pasted: several lines, possibly very long (a pasted log).

- **Keep `on_input_changed` quick.** Store the text, do the work in a task, and keep the `Task` in
  the tool, so a newer input replacing it cancels the old work:

  ```rust
  fn on_input_changed(&mut self, input: &Input, cx: &mut Context<Self>) {
      let text = input.text.clone();
      self.converting = Some(cx.spawn(async move |this, cx| {
          let output = convert(&text);
          this.update(cx, |this, cx| {
              this.output = output;
              cx.notify();
          })
          .ok();
      }));
  }
  ```

- **Wait before you fetch.** A tool that asks a server waits until typing pauses (the DNS lookup
  waits 400 ms) with `cx.background_executor().timer(…)` in that task.
- **Show something at once**: "Looking up example.com…" while it works, and the error in words when
  it fails. The tool is the only place the user sees what happened.

### Several tools in one plugin

`open_tool` returns an `AnyTool`, so each operation can have its own view type:

```rust
fn open_tool(&mut self, operation: WeatherOperation, cx: &mut App) -> AnyTool {
    match operation {
        WeatherOperation::Forecast => cx.new(|cx| ForecastView::new(self.client.clone(), cx)).into(),
        WeatherOperation::Radar => cx.new(|_| RadarView::default()).into(),
    }
}
```

Work several tools need belongs to the plugin: Formats parses the input as JSON once in `detect`,
for all six of its tools. State the tools share (a signed-in account, a list fetched once) is an
`Entity` the plugin holds and hands to each view.

## Actions

The footer's buttons: a fieldless enum, offered from `list_actions`, handed back to
`perform_action`.

```rust
#[derive(Actions)]
enum JsonAction {
    Copy,
    CopyMinified,
}

fn list_actions(&self, _: &App) -> Vec<Action<JsonAction>> {
    vec![
        Action::new(JsonAction::Copy, "Copy formatted", Shortcut::Enter).primary(),
        Action::new(JsonAction::CopyMinified, "Copy minified", Shortcut::CmdEnter),
    ]
}
```

### Keys

Every tool's actions use the same keys:

| `Shortcut` | Key | Use for |
|---|---|---|
| `Enter` | ↵ | The main thing to do here |
| `CmdEnter` | ⌘↵ | The second |
| `Option(1)` … `Option(9)` | ⌥1 … ⌥9 | The rest, in order |
| `ClickOnly` | none | What shouldn't happen by a slip of a key |

- Two actions with the same key: the earlier one has it. Another number in `Option(n)` gives no key.
- Keys work from the launcher's input as well as inside the tool. A key no action has goes on to
  the input (⌥1 types `¡` there).

### The footer

- **Four buttons**: the first four actions that aren't hidden. Every action's key works, shown or
  not.
- **`.primary()`**: a filled button, for the main action. **`.attention()`**: stands out, for
  something to do first (*Refresh* stale results).
- **`.hidden()`**: a key with no button, for keys a tool wants without spending a button, such as
  switching between environments. Hidden and `ClickOnly` together can't be used at all.
- **Labels are short verbs**: *Copy JSON*, *Open in Browser*.

### What actions do

- **Copy** with GPUI's clipboard, then toast and stay, so the user can copy something else:
  `cx.write_to_clipboard(ClipboardItem::new_string(text)); host(cx).toast("Copy JSON — copied to clipboard", cx);`
- **Open** a page with `host(cx).open_url`, then `host(cx).hide`: the user is going there.
- **Hand over** to another tool with `host(cx).set_input(text, cx)`: a package's page puts
  `repo <name>` in the input, and the repository tool takes over. ⌘Z undoes it.
- **Ask first** before what can't be undone, with `host(cx).confirm`. Never put a destructive
  action on plain ↵.

### Actions that change

`list_actions` is asked again whenever the tool notifies, so the actions follow the state: none
while there is no result, *Copy* once there is one, *Retry* after a failure.

- **Keep it cheap and without side effects.** It runs after every `cx.notify()` of the tool,
  hovers included. State that changes often and doesn't affect the actions (a hovered row) belongs
  in a child view, whose notifications don't reach the footer.
- **An action carries no data.** `Open(Env)` isn't possible: declare one variant per action
  (`OpenProduction`, `OpenStaging`) and map them.
