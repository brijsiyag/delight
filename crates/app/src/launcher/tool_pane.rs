//! A tool in the launcher: the surface its plugin draws on, and the tool object
//! that drives it. Its footer actions are asked again whenever the tool notifies.

use delight_protocol::{Action, Input, ToolApi, ToolApiCaller as _};
use delight_runtime::Plugin;
use embedded_gpui::{Remote, Surface};
use gpui::{App, AppContext as _, Context, Entity, Subscription, Task};

pub struct ToolPane {
    pub surface: Entity<Surface>,
    /// Asked whether it stopped before every call: a stopped plugin's boundary is torn down.
    plugin: Plugin,
    /// Once the plugin has opened the tool.
    tool: Option<Remote<ToolApi>>,
    /// The latest input, sent once the tool is there.
    input: Option<Input>,
    /// Whether the tool's view is shown: the tool is told when it changes, and once it arrives.
    shown: bool,
    actions: Vec<Action>,
    _opening: Task<()>,
    _observing: Option<Subscription>,
}

impl ToolPane {
    pub fn new(plugin: &Plugin, operation: &str, cx: &mut Context<Self>) -> Self {
        let surface = cx.new(Surface::new);
        let opened = plugin.open_tool(operation, &surface, cx);
        let name = plugin.manifest().plugin.name.clone();
        let opening = cx.spawn(async move |this, cx| match opened.await {
            Ok(tool) => {
                this.update(cx, |this, cx| this.attach(tool, cx)).ok();
            }
            Err(error) => log::error!("{name} didn't open its tool: {error:#}"),
        });
        Self {
            surface,
            plugin: plugin.clone(),
            tool: None,
            input: None,
            shown: false,
            actions: Vec::new(),
            _opening: opening,
            _observing: None,
        }
    }

    fn attach(&mut self, tool: Remote<ToolApi>, cx: &mut Context<Self>) {
        if self.plugin.stopped().is_some() {
            return;
        }
        if let Some(input) = self.input.clone() {
            drop(tool.on_input_changed(input, cx));
        }
        if self.shown {
            drop(tool.visibility_changed(true, cx));
        }
        // The tool notifies when its actions change, and once now.
        let pane = cx.entity().downgrade();
        let observing = tool.observe(cx, move |cx| {
            pane.update(cx, |pane, cx| pane.list_actions(cx)).ok();
        });
        self._observing = Some(observing);
        self.tool = Some(tool);
    }

    /// Tell the tool what the input is now, unless it already knows.
    pub fn on_input_changed(&mut self, input: Input, cx: &mut Context<Self>) {
        if self.input.as_ref() == Some(&input) {
            return;
        }
        if let Some(tool) = self.tool.as_ref().filter(|_| self.plugin.stopped().is_none()) {
            drop(tool.on_input_changed(input.clone(), cx));
        }
        self.input = Some(input);
    }

    /// The plugin instance the tool runs in.
    pub fn plugin(&self) -> &Plugin {
        &self.plugin
    }

    /// Show the tool, or stop showing it, and tell it (`visibility_changed`). A hidden tool keeps
    /// its state and its last picture, but draws nothing and gets no input (`Surface::set_hidden`).
    /// A plugin built before plugin API 0.3 has no such method: its error is dropped.
    pub fn set_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        // TEMPORARY(fork_hidden_surfaces): `set_hidden` is the fork's, until upstream embedded_gpui
        // can hide a surface (docs/development.md, "The embedded_gpui fork").
        self.surface.update(cx, |surface, cx| surface.set_hidden(!shown, cx));
        if self.shown == shown {
            return;
        }
        self.shown = shown;
        if let Some(tool) = self.tool.as_ref().filter(|_| self.plugin.stopped().is_none()) {
            drop(tool.visibility_changed(shown, cx));
        }
    }

    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    pub fn perform(&self, action: &str, cx: &mut App) {
        if let Some(tool) = self.tool.as_ref().filter(|_| self.plugin.stopped().is_none()) {
            drop(tool.perform_action(action.to_string(), cx));
        }
    }

    /// The user clicked elsewhere in the launcher: the tool closes its menus.
    pub fn focus_lost(&self, cx: &mut Context<Self>) {
        if let Some(tool) = self.tool.as_ref().filter(|_| self.plugin.stopped().is_none()) {
            drop(tool.focus_lost(cx));
        }
    }

    fn list_actions(&mut self, cx: &mut Context<Self>) {
        let Some(tool) = self.tool.as_ref().filter(|_| self.plugin.stopped().is_none()) else { return };
        let listed = tool.list_actions(cx);
        cx.spawn(async move |this, cx| {
            let Ok(actions) = listed.await else { return };
            this.update(cx, |this, cx| {
                if this.actions != actions {
                    this.actions = actions;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}
