# Settings

A plugin's settings appear on its page in Delight's settings window, under its tools, tips and
permissions, as **titled cards** like those. You choose the sections and draw what is in each; the
app draws every title, card and footer note, so a plugin's settings sit among the app's own.

## What you write

Return sections from `Plugin::settings_sections`. Each one has an id, a title, a view, the
view's height in pixels, and optionally a footer note:

```rust
fn settings_sections(&mut self, cx: &mut App) -> Vec<SettingsSection> {
    let account = cx.new(|cx| AccountSection::new(self.account.clone(), cx));
    vec![
        SettingsSection::new("account", "Account", ROW_HEIGHT, account)
            .footer("Delight reads your forecasts' history (read-only)."),
    ]
}
```

- **No sections, no settings.** The default is an empty list, and the page shows no cards.
- **One card per section**, in the order you return them. Use several when the topics differ
  (one card for each of two API keys), rather than one long card.
- **The `id` names the section for you.** The app keeps a section's surface while its id
  stays, and draws the content of a new id when it appears.
- **The footer** is small muted text under the card. Empty for none. Use it for what a card
  does not say by itself (where a secret is kept, what signing out does).

The app calls `settings_sections` when the page first shows, and again whenever you call
`settings_changed(cx)`. It also calls it each time it draws one section's content, and keeps only
the view of the section it wanted: the rest are dropped. After `settings_changed`, a section whose
id is already on the page keeps the view it has; a new view returned for that id isn't used. So:

> **Views are made anew each time the page opens.** Keep what a section shows in an entity the
> plugin holds (a field on your plugin type, or a global), and let the view observe it. State kept
> only in the view is lost when the page is left: one shared entity, and a thin view over it.

## What the app draws, and what you draw

| The app | You |
|---|---|
| The section's title, above the card | The rows inside the card |
| The card: a slight tint of the text colour over the page, no border, rounded | Hairlines between your rows |
| The footer note under it | Nothing else: no title, no card of your own |

Your rows sit on the app's card, so they follow [the theme](ui.md#use-the-theme-always) like a
tool's view: text is the theme's `text` unless you colour it, `text_muted` for a detail line, `fill`
with `text` for a button, `separator` for the hairlines. Don't draw a background of your own over the
card.

A row like the app's: 41 px tall, 14 px from the card's sides, the title on the left and a control
on the right, a 1 px hairline above every row but the first.

## Heights

A surface cannot say how tall its content is, so **you declare it**, and a wrong number is
visible: too tall leaves a gap in the card; too short cuts the content off.

```rust
const ROW_HEIGHT: f32 = 41.;

/// Rows of `ROW_HEIGHT`, with a 1 px hairline between two.
fn rows_height(rows: usize) -> f32 {
    rows as f32 * ROW_HEIGHT + rows.saturating_sub(1) as f32
}
```

- Draw rows of a fixed height, and compute the section's height from the same data that decides
  the rows (`hosts.len()`), in one place, so they cannot disagree.
- **Avoid content that wraps.** Text that flows onto a second line when the window is narrow needs
  a height for both cases, and you get a gap in one of them. Keep each row to one line
  (`.truncate()`), or a fixed number of rows.

### When the list or a height changes

Call `delight_plugin_api::settings_changed(cx)` when a section is added or removed, or its
height, title or footer changes: a host added to a list, an error row appearing. The app then
asks `settings_sections` again, keeps the cards whose ids remain, and resizes them.

You do **not** call it for anything inside a card that keeps its size: a toggle flipping, text
being typed, a label changing. The section's view redraws itself when its entity notifies.

## Controls and text fields

Draw them as a tool does ([Controls](ui.md#controls)): only the rectangle and its height are the
app's. Handlers that change your state use `cx.listener(...)` on the view, then update the shared
entity and `cx.notify()`.

A text field of your own needs a window, and a section's view is made without one: make the field
the first time the view draws, and keep it in the view. Sections don't scroll inside the plugin
(the app scrolls the page), so don't put a scroll container of your own around a field: GPUI's
branch panics when a surface with a field in it scrolls. Text that changes next to a field has made
it panic too: give a field a section of its own.

## Saving what the user enters

The settings window keeps nothing for you: save choices with `host(cx).set_settings`, keys and
tokens as secrets, and anything larger in `/data` ([Saving data](host-api.md#saving-data)).

## Checklist

1. State lives in a shared entity; each section view observes it.
2. One `SettingsSection` per topic, an id that never changes, a height you computed.
3. Rows of a fixed height, hairlines between them. No card or title of your own.
4. `settings_changed(cx)` when the list, a title, a footer or a height changes.
5. Every colour and the text style from the theme; looked at in light and in dark.
6. Text fields made when the view first draws, in a section of their own.
7. Test the logic in unit tests ([Test and debug](getting-started.md#test-and-debug)), not by
   sending keys to the running app.

## An example

A list of hosts, each with a Remove button. `Hosts` is the entity the plugin holds;
`HostsSection` draws the card.

```rust
fn sections(hosts: &Entity<Hosts>, cx: &mut App) -> Vec<SettingsSection> {
    let rows = hosts.read(cx).list.len();
    let view = cx.new(|cx| HostsSection::new(hosts.clone(), cx));
    vec![SettingsSection::new("hosts", "Hosts", rows_height(rows), view).footer("Searched in this order.")]
}

struct HostsSection {
    hosts: Entity<Hosts>,
    _observe: Subscription,
}

impl HostsSection {
    fn new(hosts: Entity<Hosts>, cx: &mut Context<Self>) -> Self {
        let _observe = cx.observe(&hosts, |_, _, cx| cx.notify());
        HostsSection { hosts, _observe }
    }
}

impl Render for HostsSection {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(t) = theme(cx).cloned() else { return div() };
        let rows = self.hosts.read(cx).list.clone().into_iter().enumerate().map(|(index, host)| {
            let remove = div()
                .id(("remove", index))
                .px(px(8.))
                .py(px(2.))
                .rounded(px(t.radius))
                .bg(Hsla::from(t.fill))
                .text_color(t.text)
                .child("Remove")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.hosts.update(cx, |hosts, cx| {
                        hosts.list.remove(index);
                        cx.notify();
                    });
                    settings_changed(cx); // one row fewer: the card's height changes
                }));
            div()
                .h(px(ROW_HEIGHT))
                .px(px(14.))
                .flex()
                .items_center()
                .justify_between()
                .when(index > 0, |row| row.border_t_1().border_color(t.border))
                .child(host)
                .child(remove)
        });
        div().text_size(px(t.text_size)).text_color(t.text).children(rows)
    }
}
```

## Known limits

- The height is declared, not measured (above). Removing it means a surface reporting its
  content height, which needs a change in embedded_gpui.
- The settings window is a fixed size; a card is as wide as its page. Do not depend on an exact
  width.
- A section cannot be collapsed or reordered by the user.
