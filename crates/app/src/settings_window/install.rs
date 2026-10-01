//! Installing plugins from the sidebar: the Install Plugin… button picks plugin files; its arrow
//! opens a menu of the two ways, from files or from a link. What installing them shows and does is
//! the install window's.

use delight_ui::{Icon, IconName, Theme, h_flex, v_flex};
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, PathPromptOptions,
    StatefulInteractiveElement as _, Styled as _, Window, deferred, div, prelude::FluentBuilder as _, px,
};

use super::SettingsWindow;
use crate::install_window;

impl SettingsWindow {
    /// Pick plugin files (one or several); the install window shows what each would add.
    pub(super) fn pick_plugin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Install".into()),
        });
        cx.spawn_in(window, async move |_, cx| {
            let Ok(Ok(Some(files))) = picked.await else { return };
            cx.update(|_, cx| install_window::open_files(files, cx)).ok();
        })
        .detach();
    }

    /// The sidebar's Install Plugin… button: its main part picks files, and its arrow opens the menu
    /// of the ways to install, above it. A click anywhere else closes the menu.
    pub(super) fn install_button(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let open = self.install_menu;
        let (hover, pressed) = (t.fill_subtle(), t.fill);
        let main = h_flex()
            .id("install-plugin")
            .h_full()
            .pl(px(8.))
            .pr(px(7.))
            .items_center()
            .gap(px(5.))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .child(Icon::new(IconName::Plus).size(px(12.)))
            .child("Install Plugin")
            .on_click(cx.listener(|this, _, window, cx| {
                this.install_menu = false;
                this.pick_plugin(window, cx);
            }));
        let arrow = h_flex()
            .id("install-menu")
            .w(px(20.))
            .h_full()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .when(open, |arrow| arrow.bg(pressed))
            .when(!open, |arrow| arrow.hover(move |style| style.bg(hover)))
            .child(Icon::new(IconName::ChevronDown).size(px(10.)))
            .on_click(cx.listener(|this, _, _, cx| {
                this.install_menu = !this.install_menu;
                cx.notify();
            }));
        // Small, like the controls under a list in System Settings: a hairline border, no fill.
        let split = h_flex()
            .h(px(24.))
            .rounded(px(6.))
            .border_1()
            .border_color(t.border)
            .overflow_hidden()
            .text_size(px(12.))
            .child(main)
            .child(div().w(px(1.)).h_full().bg(t.border))
            .child(arrow);
        let menu = open.then(|| {
            // Compact, as a macOS menu's items: a small icon, the title and its detail close together.
            let item = |id: &'static str, icon: IconName, title: &'static str, detail: &'static str| {
                h_flex()
                    .id(id)
                    .gap(px(8.))
                    .px(px(7.))
                    .py(px(4.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .hover(move |style| style.bg(pressed))
                    .child(Icon::new(icon).size(px(14.)).color(t.text_muted))
                    .child(
                        v_flex()
                            .child(div().text_size(px(12.5)).line_height(px(16.)).child(title))
                            .child(div().text_size(px(10.5)).line_height(px(13.)).text_color(t.text_muted).child(detail)),
                    )
            };
            let files = item("install-from-files", IconName::File, "From a File…", ".wasm files on this Mac").on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.install_menu = false;
                    this.pick_plugin(window, cx);
                    cx.notify();
                }),
            );
            let link = item("install-from-link", IconName::Globe, "From a Link…", "Plugins published at a URL").on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.install_menu = false;
                    install_window::open_link(cx);
                    cx.notify();
                }),
            );
            // Above the button, over the sidebar: drawn after the rest of the window.
            deferred(
                v_flex()
                    .absolute()
                    .left(px(0.))
                    .right(px(0.))
                    .bottom(px(30.))
                    .p(px(4.))
                    .gap(px(1.))
                    .rounded(px(8.))
                    .bg(t.surface)
                    .border_1()
                    .border_color(t.border)
                    .shadow_lg()
                    .occlude()
                    .child(files)
                    .child(link),
            )
            .with_priority(1)
        });
        h_flex()
            .relative()
            .justify_center()
            .mt(px(8.))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if this.install_menu {
                    this.install_menu = false;
                    cx.notify();
                }
            }))
            .child(split)
            .children(menu)
            .into_any_element()
    }
}
