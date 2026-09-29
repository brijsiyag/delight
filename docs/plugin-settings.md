# Designing a plugin's settings

A plugin's settings appear on its page in Delight's settings window, under its tools, tips and
permissions, as **titled cards** like those. You choose the sections and what is in each; the
app draws every title, card and footer note, so a plugin's settings look like the app's own.

This is written from building the settings of Google Calendar (a sign-in), Logs (API keys) and a
throwaway DNS page that used every kind of control. Read it with `plugins/`'s and
`delight-plugins/`'s `settings.rs` files open.

## What you write

Return sections from `Plugin::settings_sections`. Each one has an id, a title, a view, the
view's height in pixels, and optionally a footer note:

```rust
fn settings_sections(&mut self, cx: &mut App) -> Vec<SettingsSection> {
    let account = cx.new(|cx| AccountSection::new(self.calendar.clone(), cx));
    vec![
        SettingsSection::new("account", "Google account", ROW_HEIGHT, account)
            .footer("Delight reads your calendar's events (read-only)."),
    ]
}
```

- **No sections, no settings.** The default is an empty list, and the page shows no cards.
- **One card per section**, in the order you return them. Use several when the topics differ
  (Logs has "Meesho API key" and "Valmo API key"), rather than one long card.
- **The `id` names the section for you.** The app keeps a section's surface while its id
  stays, and draws the content of a new id when it appears.
- **The footer** is small muted text under the card. Empty for none. Use it for what a card
  does not say by itself (where a secret is kept, what signing out does).

The app calls `settings_sections` when the page first shows, and again whenever you call
`settings_changed(cx)`. It also calls it each time it draws one section's content, so:

> **Views are made anew each time the page opens.** Keep what a section shows in an entity the
> plugin holds (a field on your plugin type, or a global), and let the view observe it. State kept
> only in the view is lost when the page is left. Logs' `Keys` and Calendar's `Calendar` are the
> pattern: one shared entity, and a thin view over it.

## What the app draws, and what you draw

| The app | You |
|---|---|
| The section's title, above the card | The rows inside the card |
| The card: a slight tint of the text colour over the page, no border, rounded | Hairlines between your rows |
| The footer note under it | Nothing else: no title, no card of your own |

Do **not** wrap your rows in another card (`Group`): that draws a second box inside the app's.

Use `delight_ui`'s `Rows` for the hairlines, and `row` for a row:

```rust
Rows::new()
    .child(row("Signed in as ada@example.com", None, sign_out_button, &t))
    .child(row("Time limit", Some("Per server"), segmented_control, &t))
```

- `row(title, detail, control, &t)`: the title (and an optional small line under it) on the
  left, a control on the right. A `None` detail is a plain row.
- `row_with(title, detail_text, control, color)`: the same with a detail line made when it draws
  (an error, a status), in a colour of yours. It wraps to two lines; a longer one is cut off.
- Your own row (a text field with its buttons, an icon with a label): make it
  `h_flex().h(px(ROW_HEIGHT)).items_center().gap(px(8.)).px(px(14.))` so it lines up with the
  others.

`Rows` also sets the app's text size and colour. Without it a plugin's text is GPUI's default,
which is larger and black, and unreadable on a dark card.

## Heights

A surface cannot say how tall its content is, so **you declare it**, and a wrong number is
visible: too tall leaves a gap in the card; too short cuts the content off.

- A `row` is exactly `ROW_HEIGHT` (41 px), or `ROW_DETAIL_HEIGHT` (61 px) with a detail line.
- Between rows there is a 1 px hairline. Add them up with `rows_height(&[..])`:

```rust
rows_height(&[ROW_DETAIL_HEIGHT; 6])          // six switches with descriptions: 371
rows_height(&[ROW_HEIGHT, ROW_HEIGHT])        // a status row and a field row: 83
```

- Compute the height from the same data that decides the rows (`servers.len() + 1`), in one
  place, so they cannot disagree.
- **Avoid content that wraps.** Toggles that flow onto a second line when the window is narrow
  need a height for both cases, and you get a gap in one of them. Prefer one line, or a fixed
  number of rows.

### When the list or a height changes

Call `delight_plugin_api::settings_changed(cx)` when a section is added or removed, or its
height, title or footer changes: a server added to a list, an error row appearing. The app then
asks `settings_sections` again, keeps the cards whose ids remain, and resizes them. See the
DNS example below (Add / Remove) and Calendar (the account card, when an error row appears).

You do **not** call it for anything inside a card that keeps its size: a switch flipping, text
being typed, a label changing. The section's view redraws itself when its entity notifies.

## Controls that work

All from `delight_ui`, drawn in the app's look:

- `Switch::new(id).checked(bool).on_change(handler)`
- `SegmentedControl::new(id).options([..]).selected(i).on_change(handler)`
- `Button::new(id, label)`, `.primary()`, `.disabled(bool)`, `.on_click(handler)`
- `Icon::new(IconName::…)` (install `delight_ui::Assets` in `Plugin::assets` if you draw icons)
- `TextEditor` for text fields (below)
- Any GPUI element: a plugin may draw whatever it wants. Only the rectangle and its height are
  the app's.

Handlers that change your state use `cx.listener(...)` on the view, then update the shared
entity and `cx.notify()`.

### Text fields

`TextEditor::new(window, cx)` needs a window, and a section's view is created without one. So
**create the field the first time the view draws**, and keep it in the view:

```rust
fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let field = self.field.get_or_insert_with(|| {
        let field = cx.new(|cx| TextEditor::new(window, cx).placeholder("Paste the API key"));
        cx.observe(&field, |_, _, cx| cx.notify()).detach(); // typing enables Save
        field
    }).clone();
    …
}
```

Put it in the usual frame: `h_flex().h(px(28.)).px(px(8.)).rounded(px(7.)).bg(t.surface)
.border_1().border_color(t.border)`. Editing keys and ⌘V work in a plugin because
`delight_ui::init_plugin(cx)` (call it in `Plugin::new`) binds them.

A `TextEditor` next to other text on a page that **scrolls inside the plugin** currently
panics GPUI when scrolled. Sections do not scroll inside the plugin (the app scrolls the page),
so this does not arise; do not put a scroll container of your own around a field.

### Reading the theme

`cx.theme()` (`delight_ui::ActiveTheme`) gives colours (`text`, `text_muted`, `text_faint`,
`surface`, `card()`, `border`, `accent`, `success`, `warning`, `error`), sizes
(`text_size`, `text_size_small()`, `mono_font`) and `radius`. Use them rather than fixed
colours, so light and dark both work. The theme follows the app and redraws you when it changes.

## Saving what the user enters

The settings window has no store for your settings; a plugin keeps its own:

- **Secrets** (API keys, tokens, an account): `host(cx).secret(key, cx)` and
  `host(cx).set_secret(key, value, cx)` (an empty value deletes). Kept encrypted, with the key in
  the login Keychain, per plugin. Each is a task: read once at start, hold the result in your
  shared entity.
- **Settings**: a small JSON value the app keeps for the plugin, read and written as a type of
  your own (`#[derive(Serialize, Deserialize)]`): `host(cx).settings::<Saved>(cx)` gives
  `Ok(None)` if nothing was saved, `host(cx).set_settings(&saved, cx)` replaces it, and
  `host(cx).clear_settings(cx)` removes it (at most 256 KiB). A saved value that no longer fits the
  type is an error, not a default: `unwrap_or_default()` starts over when your type changed.
  Calendar keeps the signed-in account this way.
- **Files**: your data folder, mounted at `/data`, for anything bigger.
- Deleting the plugin removes all three.

## Checklist

1. `Plugin::new` calls `delight_ui::init_plugin(cx)`.
2. State lives in a shared entity; each section view observes it.
3. One `SettingsSection` per topic, an id that never changes, a height you computed.
4. Rows are `Rows` + `row`/`row_with` (or a custom row `ROW_HEIGHT` tall). No `Group`, no title
   of your own.
5. `settings_changed(cx)` when the list, a title, a footer or a height changes.
6. Text fields made in `render`, inside the 28 px frame.
7. Colours and sizes from `cx.theme()`.
8. Test behaviour headless (the plan's test crate), not by sending keys to the GUI.

Rebuild the plugin whenever the plugin API changes: an installed `.wasm` keeps the protocol it was
built with (the app logs `PluginApi has no method …` for one built before a call existed).

## A whole example

Six sections with every control, as the throwaway DNS page had them (removed since; kept here as
a reference). `Preview` is the shared entity; `PreviewSection` draws each card.

```rust
pub fn sections(model: &Entity<Preview>, cx: &mut App) -> Vec<SettingsSection> {
    let servers = model.read(cx).servers.len();
    let section = |cx: &mut App, kind, id, title: &str, height| {
        let view = cx.new(|cx| PreviewSection {
            model: model.clone(), kind, field: None,
            _observe: cx.observe(model, |_, _, cx| cx.notify()),
        });
        SettingsSection::new(id, title, height, view)
    };
    vec![
        section(cx, Kind::Lookups, "lookups", "Lookups", rows_height(&[ROW_DETAIL_HEIGHT; 6])),
        section(cx, Kind::Scope, "scope", "Scope", rows_height(&[ROW_DETAIL_HEIGHT; 2])),
        section(cx, Kind::Types, "types", "Record types", 58.),               // one line of toggles
        section(cx, Kind::Servers, "servers", "Extra DNS servers",
                rows_height(&vec![ROW_HEIGHT; servers + 1]))                     // a row each + the field
            .footer("Tried after the Mac's own resolvers, in this order."),
        section(cx, Kind::Recent, "recent", "Recent lookups", rows_height(&[ROW_HEIGHT; 12])),
        section(cx, Kind::Maintenance, "maintenance", "Maintenance",
                rows_height(&[ROW_DETAIL_HEIGHT, ROW_DETAIL_HEIGHT, ROW_HEIGHT])),
    ]
}
```

A switch row, a segmented-control row, a removable list row and the add-a-server row:

```rust
// Lookups: a switch per row, each with a description.
let control = Switch::new(("switch", i)).checked(state.switches[i]).on_change(
    cx.listener(move |this, on: &bool, _, cx| {
        this.model.update(cx, |model, cx| { model.switches[i] = *on; cx.notify(); });
    }));
row(title, Some(detail), control, &t)

// Scope: a segmented control.
let scope = SegmentedControl::new("scope").options(["Common", "All", "Custom"])
    .selected(state.scope)
    .on_change(cx.listener(|this, index: &usize, _, cx| {
        this.model.update(cx, |model, cx| { model.scope = *index; cx.notify(); });
    }));
row("Record types", Some("Which are asked for"), scope, &t)

// Extra DNS servers: a row per server with Remove …
let remove = Button::new(("remove-server", i), "Remove").on_click(cx.listener(move |this, _, _, cx| {
    this.model.update(cx, |model, cx| { model.servers.remove(i); cx.notify(); });
    settings_changed(cx);                     // the card is one row shorter
}));
row(server.clone(), None, remove, &t)

// … and a last row: the field and Add.
let add = Button::new("add-server", "Add").disabled(typed.is_empty()).on_click(cx.listener(move |this, _, _, cx| {
    let server = field.read(cx).text().trim().to_string();
    if !server.is_empty() {
        this.model.update(cx, |model, cx| { model.servers.push(server); cx.notify(); });
        field.update(cx, |field, cx| field.set_text("", cx));
        settings_changed(cx);                 // one row taller
    }
}));
h_flex().h(px(ROW_HEIGHT)).items_center().gap(px(8.)).px(px(14.)).child(entry).child(add)
```

A card's content is a `Rows::new().children(rows)`, or any element for a custom card (the
toggles were an `h_flex().flex_wrap()`, which is why their height needed care).

## Known limits

- The height is declared, not measured (above). Removing it means a surface reporting its
  content height, which needs a change in embedded_gpui.
- The settings window is a fixed size; a card is as wide as its page. Do not depend on an exact
  width.
- A section cannot be collapsed or reordered by the user.
