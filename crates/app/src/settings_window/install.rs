//! Installing a plugin: pick its `.wasm`, read its manifest without running any of it,
//! and show what it is, which permissions it has and which tools it adds before
//! anything is copied.

use std::path::PathBuf;

use delight_protocol::Manifest;
use delight_ui::{Button, LogoBadge, Theme, h_flex, v_flex};
use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement, PathPromptOptions, PromptLevel, Styled, Window, div,
    hsla, prelude::*, px,
};

use super::plugins::{file_name, permission_rows};
use super::{Page, SettingsWindow, item};
use crate::plugins;

/// The most of the window's height the sheet takes.
const SHEET_MAX_HEIGHT: f32 = 0.7;

/// A plugin picked to install, waiting for Install or Cancel.
pub(super) struct Pending {
    file: PathBuf,
    manifest: Manifest,
}

impl SettingsWindow {
    /// Pick a plugin file, then show what installing it would add.
    pub(super) fn pick_plugin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Install".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(mut files))) = picked.await else { return };
            let Some(file) = files.pop() else { return };
            let read = this.update(cx, |_, cx| plugins::inspect(file.clone(), cx));
            let Ok(read) = read else { return };
            let manifest = read.await;
            this.update_in(cx, |this, window, cx| match manifest {
                Ok(manifest) => {
                    this.installing = Some(Pending { file, manifest });
                    cx.notify();
                }
                Err(error) => {
                    let detail = format!("{} isn’t a plugin this Delight can run: {error:#}", file_name(&file));
                    // Only OK: nothing waits for the answer.
                    drop(window.prompt(PromptLevel::Warning, "Not installed", Some(&detail), &["OK"], cx));
                }
            })
            .ok();
        })
        .detach();
    }

    fn install(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending) = self.installing.take() else { return };
        match plugins::install(&pending.file, &pending.manifest, cx) {
            // It shows once the plugins have started again.
            Ok(()) => self.page = Page::Plugin(pending.manifest.plugin.id.clone()),
            Err(error) => {
                let detail = format!("{error:#}");
                drop(window.prompt(PromptLevel::Critical, "Not installed", Some(&detail), &["OK"], cx));
            }
        }
        cx.notify();
    }

    pub(super) fn cancel_install(&mut self, cx: &mut Context<Self>) {
        self.installing = None;
        cx.notify();
    }

    /// The sheet over the window: what the plugin is, its permissions and its tools.
    pub(super) fn render_install(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pending = self.installing.as_ref()?;
        let properties = &pending.manifest.plugin;
        let operations = &pending.manifest.operations;
        // What it replaces, if it has the id of a plugin there is.
        let replaces = plugins::all(cx)
            .iter()
            .zip(plugins::sources(cx).iter())
            .find(|(plugin, _)| plugin.manifest().plugin.id == properties.id)
            .map(|(plugin, source)| {
                let kind = if source.built_in { "built-in" } else { "installed" };
                let old = &plugin.manifest().plugin;
                format!("It replaces the {kind} “{}” {}.", old.name, old.version)
            });

        let tools = operations
            .iter()
            .map(|operation| {
                let icon = operation.icon.as_deref().unwrap_or(&properties.icon);
                item(
                    LogoBadge::new(icon.as_bytes()).size(px(30.)).into_any_element(),
                    operation.title.clone().into(),
                    Some(operation.description.clone().into()),
                    None,
                    t,
                )
            })
            .collect();

        let header = h_flex()
            .flex_shrink_0()
            .gap(px(14.))
            .child(LogoBadge::new(properties.icon.as_bytes()).size(px(52.)))
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Install “{}”?", properties.name)),
                    )
                    .child(
                        div()
                            .mt(px(2.))
                            .text_size(px(12.))
                            .text_color(t.text_muted)
                            .child(format!("{} · {}", properties.version, properties.id)),
                    ),
            );
        // Between the header and the buttons, scrolling when it's more than fits.
        let details = div().id("install-details").min_h(px(0.)).overflow_y_scroll().child(
            v_flex()
                .gap(px(14.))
                .when(!properties.description.is_empty(), |details| {
                    details.child(div().text_color(t.text_muted).child(properties.description.clone()))
                })
                .children(replaces.map(|text| div().text_size(t.text_size_small()).child(text)))
                .child(counted("Permissions", properties.permissions.len(), permission_rows(&properties.permissions, t), t))
                .child(counted("Tools", operations.len(), tools, t)),
        );
        let buttons = h_flex()
            .flex_shrink_0()
            .justify_end()
            .gap(px(8.))
            .pt(px(2.))
            .child(Button::new("cancel-install", "Cancel").on_click(cx.listener(|this, _, _, cx| this.cancel_install(cx))))
            .child(
                Button::new("install", "Install")
                    .primary()
                    .on_click(cx.listener(|this, _, window, cx| this.install(window, cx))),
            );
        let sheet = v_flex()
            .w(px(460.))
            // Never more than this of the window; less when there's less to show.
            .max_h(px(super::HEIGHT * SHEET_MAX_HEIGHT))
            .gap(px(14.))
            .p(px(20.))
            .rounded(px(12.))
            .bg(super::content_color(t))
            .border_1()
            .border_color(t.border)
            .shadow_lg()
            .child(header)
            .child(details)
            .child(buttons);
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(hsla(0., 0., 0., 0.28))
                .flex()
                .justify_center()
                // As tall as what it shows, from near the top.
                .items_start()
                .pt(px(36.))
                // Clicks stay on the sheet, not on the window under it.
                .occlude()
                .child(sheet)
                .into_any_element(),
        )
    }
}

/// A section titled with its name and, when there are any, how many it has.
fn counted(title: &'static str, count: usize, rows: Vec<AnyElement>, t: &Theme) -> impl IntoElement {
    let badge = (count > 0).then(|| div()
        .min_w(px(18.))
        .h(px(18.))
        .px(px(6.))
        .rounded(px(9.))
        .bg(t.fill)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.text_muted)
        .child(count.to_string()));
    v_flex()
        .gap(px(6.))
        .child(
            h_flex()
                .gap(px(6.))
                .px(px(2.))
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title)
                .children(badge),
        )
        .child(delight_ui::Group::new().children(rows))
}
