//! The plugin's half of the protocol: its root object and one object per open tool,
//! implemented over the author's [`Plugin`] and [`Tool`](crate::Tool)s.

use std::rc::Rc;

use anyhow::{Result, anyhow};
use delight_protocol::{Action, Detection, HostApi, Input, PluginApi, ToolApi};
use embedded_gpui::surface::SurfaceApi;
use embedded_gpui::{Ref, open_view, root, share, share_root, shared};

use crate::gpui::{
    AnyEntity, AnyView, App, AppContext as _, Context, IntoElement, ParentElement as _, Render,
    Styled as _, Subscription, Task, Window, div,
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
        let mut tool = None;
        let opened = open_view(surface, cx, |window, cx| {
            let opened = self.plugin.open_tool(operation, window, cx);
            let view = opened.view.clone();
            tool = Some(opened.tool);
            cx.new(|_| Filling { view: Some(view) })
        });
        let result = match (opened, tool) {
            (Ok(_), Some(tool)) => {
                let home = cx.new(|cx| ToolHome::new(tool, cx));
                Ok(share(&home, cx))
            }
            (Err(error), _) => Err(error.context("opening the tool's view")),
            (Ok(_), None) => Err(anyhow!("the tool's view opened without the tool")),
        };
        Task::ready(result)
    }

    fn open_settings(&mut self, surface: Ref<SurfaceApi>, cx: &mut Context<Self>) -> bool {
        let mut has_page = false;
        let opened = open_view(surface, cx, |window, cx| {
            let page = self.plugin.settings_page(window, cx);
            has_page = page.is_some();
            // Without a page the view is empty, and the app drops the surface.
            cx.new(|_| Filling { view: page })
        });
        if let Err(error) = &opened {
            log::error!("opening the settings page: {error:#}");
        }
        opened.is_ok() && has_page
    }
}

/// The window's root view: a tool, or the settings page, filling the space it's
/// given.
struct Filling {
    view: Option<AnyView>,
}

impl Render for Filling {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().flex_col().children(self.view.clone())
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
