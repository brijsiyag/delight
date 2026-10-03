//! How permissions look, in Settings and in the install window: a row for each permission, with
//! its icon, its name and the plugin's reason, that opens on a click to say what the permission
//! allows and to list its items. Each permission draws its own items ([`PermissionView`]); the
//! rows around them are the same for all.

use std::collections::HashSet;

use delight_protocol::{CommandsPermission, FilesPermission, NetworkPermission, Permission, PermissionRequest, Permissions};
use delight_ui::{Icon, IconButton, IconName, Sizable as _, StyledExt as _, Theme, h_flex, v_flex};
use gpui::{AnyElement, App, ClickEvent, FontWeight, IntoElement, ParentElement, Styled, Window, div, prelude::*, px};

use crate::settings_window::item;

/// How a permission's items look: each permission draws its own, as it needs to.
pub(crate) trait PermissionView {
    /// Its items, in its order, each as it draws it. None for a permission that lists nothing.
    fn items(&self, t: &Theme) -> Vec<ItemView>;
}

/// One of a permission's items as the permission draws it, with its key: the item as the
/// permission holds it, which taking it back removes ([`Permission::remove`]).
pub(crate) struct ItemView {
    pub key: String,
    pub view: AnyElement,
}

/// `permission` as [`PermissionView`], whichever it is.
fn view_of(permission: &Permission) -> &dyn PermissionView {
    match permission {
        Permission::Network(network) => network,
        Permission::Commands(commands) => commands,
        Permission::Files(files) => files,
    }
}

/// It lists nothing.
impl PermissionView for NetworkPermission {
    fn items(&self, _t: &Theme) -> Vec<ItemView> {
        Vec::new()
    }
}

/// Each program it runs, as a pill.
impl PermissionView for CommandsPermission {
    fn items(&self, t: &Theme) -> Vec<ItemView> {
        self.programs.iter().map(|program| ItemView { key: program.clone(), view: pill(program, t) }).collect()
    }
}

/// Each folder as a pill, with what the plugin may do there.
impl PermissionView for FilesPermission {
    fn items(&self, t: &Theme) -> Vec<ItemView> {
        self.folders()
            .map(|(folder, write)| {
                let access = div().flex_shrink_0().text_size(px(11.)).text_color(t.text_faint).child(if write { "write" } else { "read" });
                let view = h_flex().min_w(px(0.)).gap(px(6.)).child(pill(folder, t)).child(access).into_any_element();
                ItemView { key: folder.clone(), view }
            })
            .collect()
    }
}

/// What a permission lists (a program, a folder) as a pill, in the permission's orange: one line,
/// cut with an ellipsis at the card's width, as a path can be very long.
fn pill(text: &str, t: &Theme) -> AnyElement {
    // Orange like the icon, so what the plugin has stands out; a step darker in light mode, where
    // the orange itself is too pale for text.
    let ink = if t.dark { t.warning } else { gpui::hsla(t.warning.h, t.warning.s, t.warning.l * 0.72, 1.) };
    div()
        .min_w(px(0.))
        .truncate()
        .px(px(8.))
        .py(px(2.))
        .rounded(px(10.))
        .bg(t.tint(t.warning))
        .border_1()
        .border_color(t.warning.opacity(0.4))
        .font_family(t.mono_font.clone())
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(ink)
        .child(delight_ui::ellipsize(text, 100))
        .into_any_element()
}

/// Which of a plugin's permission rows are open: none, until one is clicked.
#[derive(Debug, Default)]
pub(crate) struct OpenPermissions {
    open: HashSet<usize>,
    /// The rows whose long list shows all of it, not its first [`MOST_ITEMS`].
    all_shown: HashSet<usize>,
}

/// What a click toggles: a row, open or closed; or its long list, all of it or its start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Toggled {
    Row(usize),
    List(usize),
}

impl OpenPermissions {
    pub(crate) fn is_open(&self, index: usize) -> bool {
        self.open.contains(&index)
    }

    pub(crate) fn shows_all(&self, index: usize) -> bool {
        self.all_shown.contains(&index)
    }

    pub(crate) fn toggle(&mut self, toggled: Toggled) {
        let (set, index) = match toggled {
            Toggled::Row(index) => (&mut self.open, index),
            Toggled::List(index) => (&mut self.all_shown, index),
        };
        if !set.remove(&index) {
            set.insert(index);
        }
    }
}

/// How many of a permission's items an open row shows before "Show N more".
const MOST_ITEMS: usize = 8;

/// What a click on a permission's row does: the view that draws the rows makes one for each.
pub(crate) type Toggle = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// What a click on a given item's remove button does, made for each from the permission it is of
/// (its kind) and its key.
pub(crate) type TakeBack<'a> = dyn Fn(&Permission, &str) -> Toggle + 'a;

/// What the user gave a plugin beyond its manifest (a folder, say), which a permission's row lists
/// after its manifest's items.
pub(crate) struct Given<'a> {
    pub permissions: Permissions,
    /// None where nothing can be taken back (the install window: nothing is given yet).
    pub take_back: Option<&'a TakeBack<'a>>,
}

impl Given<'_> {
    /// Nothing given: a plugin that isn't installed yet.
    pub fn none() -> Given<'static> {
        Given { permissions: Permissions::default(), take_back: None }
    }
}

/// A line under an open permission: one of its items, and a remove button for one the user gave
/// the plugin.
struct Line {
    item: ItemView,
    take_back: Option<Toggle>,
}

/// The rows of a Permissions section: one per permission. Each shows an icon, the permission's name
/// (with how many things it lists, if it lists any) and the plugin's own reason in one line; click
/// it (or its chevron) for the reason in full, what the permission allows, and, for a permission
/// that lists things (the programs `Commands` runs, the folders `Files` has), each on a line of its
/// own: the first [`MOST_ITEMS`] of a long list, then "Show N more". Or one row saying there are none.
pub(crate) fn permission_rows(
    permissions: &[PermissionRequest],
    given: &Given,
    open: &OpenPermissions,
    toggle: impl Fn(Toggled) -> Toggle,
    t: &Theme,
) -> Vec<AnyElement> {
    if permissions.is_empty() {
        let check = Icon::new(IconName::CircleCheck).size(px(18.)).color(t.success).into_any_element();
        return vec![item(check, "Needs no permissions".into(), None, None, t)];
    }
    let last = permissions.len() - 1;
    permissions
        .iter()
        .enumerate()
        .map(|(index, request)| {
            let shown = Shown { open: open.is_open(index), all: open.shows_all(index) };
            permission_row(index, index == last, request, given, shown, toggle(Toggled::Row(index)), toggle(Toggled::List(index)), t)
        })
        .collect()
}

/// How much of a permission's row shows: whether it is open, and all of its list.
struct Shown {
    open: bool,
    all: bool,
}

/// One permission's row: `index` in the card, and whether it is the `last` (the card's corners are
/// its own there). `toggle` opens and closes it; `show_all` shows all of a long list, or its start.
#[allow(clippy::too_many_arguments)]
fn permission_row(index: usize, last: bool, request: &PermissionRequest, given: &Given, shown: Shown, toggle: Toggle, show_all: Toggle, t: &Theme) -> AnyElement {
    let open = shown.open;
    let spec = request.permission.spec();
    let icon = IconName::from_name(spec.icon()).unwrap_or(IconName::Puzzle);
    // Just the icon, in the permission's colour: no tile behind it.
    let tile = div().size(px(20.)).flex_shrink_0().flex().items_center().justify_center().child(Icon::new(icon).size(px(18.)).color(t.warning));
    // The reason: one line when the row is closed, all of it (at most a sentence) when it is open.
    let why = div().mt(px(2.)).text_size(px(12.)).clamp_lines(if open { 4 } else { 1 }).child(request.reason.clone());
    let allows = open.then(|| {
        v_flex()
            .mt(px(8.))
            .gap(px(1.))
            .child(div().text_size(px(10.)).font_weight(FontWeight::BOLD).text_color(t.text_faint).child("ALLOWS"))
            .child(div().text_size(px(12.)).text_color(t.text_muted).clamp_lines(3).child(spec.describe()))
    });
    // Its manifest's items, then those the user gave it, which they can take back; counted together.
    let mut lines: Vec<Line> = view_of(&request.permission).items(t).into_iter().map(|item| Line { item, take_back: None }).collect();
    let mut all = request.permission.clone();
    if let Some(more) = given.permissions.like(&request.permission) {
        lines.extend(view_of(more).items(t).into_iter().map(|item| Line {
            take_back: given.take_back.map(|take_back| take_back(more, &item.key)),
            item,
        }));
        all.add(more);
    }
    let count = all.spec().count_items();
    // A long list shows its start, then "Show N more"; shown all, "Show less" folds it again.
    let hidden = if shown.all { 0 } else { lines.len().saturating_sub(MOST_ITEMS) };
    let fold = (lines.len() > MOST_ITEMS).then(|| {
        let label = if hidden > 0 { format!("Show {hidden} more") } else { "Show less".to_string() };
        let accent = t.accent;
        div()
            .id(("show-all", index))
            .mt(px(2.))
            .text_size(px(12.))
            .text_color(accent)
            .cursor_pointer()
            .hover(|link| link.opacity(0.8))
            .on_click(show_all)
            .child(label)
    });
    let shown_lines = lines.len() - hidden;
    let items = (open && !lines.is_empty()).then(|| {
        // One per line, so a long list of paths reads down the card.
        v_flex().items_stretch().gap(px(4.)).mt(px(7.)).children(lines.into_iter().take(shown_lines).enumerate().map(|(place, line)| {
            h_flex().gap(px(6.)).child(line.item.view).child(div().flex_1()).children(line.take_back.map(|take_back| {
                IconButton::new(("take-back", place), IconName::X).small().tooltip("Remove: the plugin starts again without it").on_click(take_back)
            }))
        }))
        .children(fold)
    });
    let chevron = Icon::new(if open { IconName::ChevronUp } else { IconName::ChevronDown }).size(px(14.)).color(t.text_faint);
    // The icon, the name and the chevron share one line, so they line up; the text under it starts
    // at the name.
    v_flex()
        .id(("permission", index))
        .px(px(14.))
        .py(px(10.))
        // The hover colour follows the card's rounded corners at its top and bottom: GPUI clips to
        // rectangles, so the card can't round it.
        .when(index == 0, |row| row.rounded_t(t.radius))
        .when(last, |row| row.rounded_b(t.radius))
        .cursor_pointer()
        .hover(|row| row.bg(t.fill_subtle()))
        .on_click(toggle)
        .child(
            h_flex()
                .items_center()
                .gap(px(12.))
                .child(tile)
                .child(
                    h_flex()
                        .flex_1()
                        .min_w(px(0.))
                        .gap(px(6.))
                        .child(div().flex_shrink_0().font_weight(FontWeight::SEMIBOLD).child(spec.title()))
                        // While closed, how long its list is: open, the list says it.
                        .children(count.filter(|_| !open).map(|count| {
                            div().min_w(px(0.)).truncate().text_size(px(12.)).text_color(t.text_faint).child(format!("· {count}"))
                        })),
                )
                .child(div().flex_shrink_0().child(chevron)),
        )
        .child(v_flex().pl(px(32.)).child(why).children(allows).children(items))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_opens_and_its_list_shows_all_apart() {
        let mut open = OpenPermissions::default();
        open.toggle(Toggled::Row(1));
        assert!(open.is_open(1) && !open.shows_all(1));
        open.toggle(Toggled::List(1));
        assert!(open.is_open(1) && open.shows_all(1));
        open.toggle(Toggled::Row(1));
        assert!(!open.is_open(1) && open.shows_all(1), "closed, it still shows all when it opens again");
        open.toggle(Toggled::List(1));
        assert!(!open.shows_all(1));
        assert!(!open.is_open(0), "each row on its own");
    }
}
