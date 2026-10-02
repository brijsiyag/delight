//! The plugin's half of the protocol: its root object and one object per open tool,
//! implemented over the author's [`Plugin`] and [`Tool`](crate::Tool)s.

use std::rc::Rc;

use anyhow::{Result, anyhow};
use delight_protocol::{Action, Detection, HostApi, Input, PluginApi, SettingsSection, ToolApi};
use embedded_gpui::surface::SurfaceApi;
use embedded_gpui::{Ref, open_view, root, share, share_root, shared};

use crate::gpui::{
    AnyEntity, AnyView, App, AppContext as _, Context, IntoElement, ParentElement as _, Render,
    Styled as _, Subscription, Task, Window, div, px,
};
use crate::host::HostRoot;
use crate::tool::DynTool;
use crate::{Operations as _, Plugin};

/// Start the plugin: connect to the app's root (and follow its theme), build the
/// author's plugin, and install the plugin's root object. The returned entity must be
/// kept alive.
pub fn start<P: Plugin>(cx: &mut App) -> AnyEntity {
    HostRoot::connect(root::<HostApi>(), cx);
    let plugin = P::new(cx);
    let plugin_root = cx.new(|_| PluginRoot { plugin });
    cx.set_global(crate::host::RootId(plugin_root.entity_id()));
    share_root(&plugin_root, cx);
    plugin_root.into_any()
}

struct PluginRoot<P> {
    plugin: P,
}

#[shared]
impl<P: Plugin> PluginApi for PluginRoot<P> {
    fn detect(&mut self, input: Input, cx: &mut Context<Self>) -> Vec<Detection> {
        let detections = self.plugin.detect(&input, cx);
        detections
            .into_iter()
            .map(|detection| Detection {
                operation: detection.operation.id().to_string(),
                confidence: detection.confidence,
            })
            .collect()
    }

    fn open_tool(
        &mut self,
        operation: String,
        surface: Ref<SurfaceApi>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Ref<ToolApi>>> {
        let Some(operation) = P::Operation::from_id(&operation) else {
            return Task::ready(Err(anyhow!("this plugin has no operation {operation:?}")));
        };
        let opened = self.plugin.open_tool(operation, cx);
        let filling = cx.new(|_| Filling { view: Some(opened.view) });
        let result = match open_view(surface, filling, cx) {
            Ok(()) => {
                let home = cx.new(|cx| ToolHome::new(opened.tool, cx));
                Ok(share(&home, cx))
            }
            Err(error) => Err(error.context("opening the tool's view")),
        };
        Task::ready(result)
    }

    fn settings_sections(&mut self, cx: &mut Context<Self>) -> Vec<SettingsSection> {
        self.plugin
            .settings_sections(cx)
            .into_iter()
            .map(|section| SettingsSection {
                id: section.id.to_string(),
                title: section.title,
                height: section.height,
                footer: section.footer,
            })
            .collect()
    }

    fn open_window_view(&mut self, key: String, surface: Ref<SurfaceApi>, cx: &mut Context<Self>) -> bool {
        let Some(view) = cx.default_global::<crate::host::PendingWindows>().0.remove(&key) else {
            return false;
        };
        let filling = cx.new(|_| Filling { view: Some(view) });
        match open_view(surface, filling, cx) {
            Ok(()) => true,
            Err(error) => {
                log::error!("opening the window {key:?}: {error:#}");
                false
            }
        }
    }

    fn open_settings_section(&mut self, id: String, surface: Ref<SurfaceApi>, cx: &mut Context<Self>) -> bool {
        let Some(section) = self.plugin.settings_sections(cx).into_iter().find(|section| section.id == id) else {
            return false;
        };
        let filling = cx.new(|_| Filling { view: Some(section.view) });
        match open_view(surface, filling, cx) {
            Ok(()) => true,
            Err(error) => {
                log::error!("opening the settings section {id:?}: {error:#}");
                false
            }
        }
    }
}

/// The view on a surface: a tool, a settings section or a window's, filling the space it's given.
struct Filling {
    view: Option<AnyView>,
}

impl Render for Filling {
    /// With the theme's text style, so text the plugin draws without a colour of its own is the
    /// theme's text and not GPUI's default (black, 16 px), which can't be read on a dark
    /// background. Drawn again when the theme changes (`HostRoot` refreshes every window).
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div().size_full().flex().flex_col();
        let root = match crate::theme(cx) {
            Some(theme) => root.text_color(theme.text).text_size(px(theme.text_size)).font_family(theme.font.clone()),
            None => root,
        };
        root.children(self.view.clone())
    }
}

/// One open tool as the app reaches it. It notifies whenever the tool does, so the
/// app, observing it, asks for the actions again.
struct ToolHome {
    tool: Rc<dyn DynTool>,
    _changes: Subscription,
}

impl ToolHome {
    fn new(tool: Rc<dyn DynTool>, cx: &mut Context<Self>) -> Self {
        let home = cx.entity().downgrade();
        let changes = tool.observe(
            Box::new(move |cx| {
                home.update(cx, |_, cx| cx.notify()).ok();
            }),
            cx,
        );
        ToolHome {
            tool,
            _changes: changes,
        }
    }
}

#[shared]
impl ToolApi for ToolHome {
    fn on_input_changed(&mut self, input: Input, cx: &mut Context<Self>) {
        self.tool.on_input_changed(&input, cx);
    }

    fn list_actions(&mut self, cx: &mut Context<Self>) -> Vec<Action> {
        self.tool.list_actions(cx)
    }

    fn perform_action(&mut self, action: String, cx: &mut Context<Self>) {
        self.tool.perform_action(&action, cx);
    }

    fn focus_lost(&mut self, cx: &mut Context<Self>) {
        self.tool.focus_lost(cx);
    }

    fn visibility_changed(&mut self, shown: bool, cx: &mut Context<Self>) {
        self.tool.visibility_changed(shown, cx);
    }
}

/// `head` then `tail`, as one array: the plugin's custom section, joined at
/// compile time from its properties and its operations.
pub const fn concat<const N: usize>(head: &[u8], tail: &[u8]) -> [u8; N] {
    let mut joined = [0; N];
    let mut i = 0;
    while i < head.len() {
        joined[i] = head[i];
        i += 1;
    }
    let mut j = 0;
    while j < tail.len() {
        joined[i + j] = tail[j];
        j += 1;
    }
    joined
}
