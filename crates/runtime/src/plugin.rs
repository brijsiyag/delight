//! Running plugins: each in its own sandbox, reached through its root object.

use std::cell::RefCell;
use std::future::Future;
use std::path::PathBuf;
use std::pin::pin;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use delight_manifest::{Manifest, NetworkPermission};
use delight_protocol::{Detection, HostApi, Input, PluginApi, PluginApiCaller as _, SettingsSection, ToolApi};
use embedded_gpui::gpui::{App, Entity, PlatformTextSystem, Subscription, Task};
use embedded_gpui::{PluginHost, PluginHostHandle as _, PluginOptions, Remote, Shared, Surface};
use futures::future::{Either, join_all, select};
use wasmtime_wasi::{DirPerms, FilePerms};

use crate::{Candidate, Granted, rank};

/// Longest the app waits for a plugin's answer. A plugin embedded_gpui stops is marked
/// stopped the moment it does (the app watches its host), and isn't called again; the
/// timeout is for one that runs but doesn't answer, and makes that a stop too.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(3);

/// How embedded_gpui's calls fail when the plugin stopped (a trap: its turn took too
/// long, it ran out of memory, or it panicked): every failed call reads "call failed:
/// {reason}", and a stop's reason starts "plugin stopped: ".
const STOPPED_BY_EMBEDDED_GPUI: &str = "call failed: plugin stopped";

/// How long a question about a plugin's text field waits for the plugin's answer.
const INPUT_QUERY_BUDGET: Duration = Duration::from_millis(20);

/// The sandbox a plugin runs in, from what its manifest grants: its data folder
/// (created if needed) at `/data`, and the network (sockets and name lookups) only
/// with [`Permission::Network`]. Nothing else outside the sandbox is reachable.
pub fn plugin_options(
    manifest: &Manifest,
    data_dir: PathBuf,
    text_system: Arc<dyn PlatformTextSystem>,
) -> PluginOptions {
    let network = manifest.plugin.permission::<NetworkPermission>().is_some();
    let name = manifest.plugin.name.clone();
    // The app asks a plugin's focused text field questions on the main thread (where the cursor is,
    // for the input method) and waits for the answer this long: the plugin's turn in between may
    // be a frame's drawing, more than embedded_gpui's default 5 ms, and an unanswered question is
    // a wrong answer.
    PluginOptions::new(text_system).with_input_query_budget(INPUT_QUERY_BUDGET).with_wasi(move |wasi| {
        // What the plugin logs goes into the app's log (see `plugin_log`), not to the terminal.
        wasi.stderr(crate::plugin_log::PluginStderr::new(name.clone()));
        let mounted = std::fs::create_dir_all(&data_dir)
            .map_err(anyhow::Error::from)
            .and_then(|()| wasi.preopened_dir(&data_dir, "/data", DirPerms::all(), FilePerms::all()));
        if let Err(error) = mounted {
            log::error!("mounting the plugin's data folder {}: {error:#}", data_dir.display());
        }
        if network {
            wasi.inherit_network().allow_ip_name_lookup(true);
        }
    })
}

/// A started plugin: its manifest, and the root object the app talks to. Clones share
/// the same plugin.
///
/// It is marked stopped, with the reason, when embedded_gpui stops it (a trap, in a call
/// or not) or a call times out (see [`CALL_TIMEOUT`]); it isn't called again, and every
/// call fails at once.
#[derive(Clone)]
pub struct Plugin {
    manifest: Rc<Manifest>,
    host: Entity<PluginHost>,
    root: Remote<PluginApi>,
    stopped: Rc<RefCell<Option<String>>>,
    /// Watching the host, to mark the plugin stopped as soon as embedded_gpui stops it: a
    /// call made after that would otherwise go out and never be answered.
    _watching: Rc<Subscription>,
}

impl Plugin {
    /// Start the plugin in `file`: compile and instantiate it on a background thread,
    /// make the app's root object for this plugin alone with `host_root`, given what
    /// the manifest grants, install it, and connect to the plugin's root. A plugin
    /// that can't start is an error. `data_dir` is the plugin's data folder (the one
    /// `options` mounts), where the programs it may run work.
    pub fn start<H: Shared<HostApi>>(
        file: PathBuf,
        manifest: Manifest,
        options: PluginOptions,
        data_dir: PathBuf,
        host_root: impl FnOnce(Granted, &mut App) -> Entity<H> + 'static,
        cx: &mut App,
    ) -> Task<Result<Plugin>> {
        let load = PluginHost::load(file, options, cx);
        cx.spawn(async move |cx| {
            let host = load.await.context("the plugin didn't start")?;
            Ok(cx.update(|cx| {
                let granted = Granted::new(&manifest, data_dir, host.registry(cx), host.read(cx).clipboard(), cx);
                let host_root = host_root(granted, cx);
                host.share_root(&host_root, cx);
                let root = host.root::<PluginApi>(cx);
                let stopped: Rc<RefCell<Option<String>>> = Rc::default();
                let marking = stopped.clone();
                let watching = cx.observe(&host, move |host, cx| {
                    if let Some(reason) = host.read(cx).stopped() {
                        marking.borrow_mut().get_or_insert_with(|| reason.to_string());
                    }
                });
                Plugin {
                    manifest: Rc::new(manifest),
                    host,
                    root,
                    stopped,
                    _watching: Rc::new(watching),
                }
            }))
        })
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Whether both are the same running instance (clones of one start), not only the same plugin:
    /// a plugin started again from a changed file is another instance with the same id.
    pub fn same_instance(&self, other: &Plugin) -> bool {
        self.host.entity_id() == other.host.entity_id()
    }

    /// Why the plugin stopped, if it did.
    pub fn stopped(&self) -> Option<String> {
        self.stopped.borrow().clone()
    }

    /// Which of the plugin's operations fit `input`, as it answers.
    pub fn detect(&self, input: &Input, cx: &mut App) -> Task<Result<Vec<Detection>>> {
        let input = input.clone();
        self.call(|root, cx| root.detect(input, cx), cx)
    }

    /// Open the tool for `operation` on `surface`, which is shared with this plugin to
    /// draw on. An operation the plugin doesn't have is an error, not a stop.
    pub fn open_tool(
        &self,
        operation: &str,
        surface: &Entity<Surface>,
        cx: &mut App,
    ) -> Task<Result<Remote<ToolApi>>> {
        let operation = operation.to_string();
        let host = self.host.clone();
        let surface = surface.clone();
        self.call(
            move |root, cx| {
                let surface = host.share(&surface, cx);
                // Answered with the tool's ref, once the plugin has opened it.
                root.open_tool(operation, surface, cx).into_future()
            },
            cx,
        )
    }

    /// The sections of the plugin's settings (see [`SettingsSection`]): none for a plugin
    /// without settings.
    pub fn settings_sections(&self, cx: &mut App) -> Task<Result<Vec<SettingsSection>>> {
        self.call(|root, cx| root.settings_sections(cx), cx)
    }

    /// Ask the plugin to draw the content of its settings section `id` on `surface`,
    /// shared with it like a tool's: whether it did. A plugin that can't answer (built
    /// before sections, or stopped) draws nothing.
    pub fn open_settings_section(&self, id: &str, surface: &Entity<Surface>, cx: &mut App) -> Task<bool> {
        let host = self.host.clone();
        let surface = surface.clone();
        let id = id.to_string();
        let asked = self.call(
            move |root, cx| {
                let surface = host.share(&surface, cx);
                root.open_settings_section(id, surface, cx)
            },
            cx,
        );
        cx.spawn(async move |_| asked.await.unwrap_or(false))
    }

    /// Ask the plugin to draw the window it asked for (`host(cx).open_window`), `key`, on `surface`:
    /// whether it did. A plugin that can't answer (stopped, or without that window) draws nothing.
    pub fn open_window_view(&self, key: &str, surface: &Entity<Surface>, cx: &mut App) -> Task<bool> {
        let host = self.host.clone();
        let surface = surface.clone();
        let key = key.to_string();
        let asked = self.call(
            move |root, cx| {
                let surface = host.share(&surface, cx);
                root.open_window_view(key, surface, cx)
            },
            cx,
        );
        cx.spawn(async move |_| asked.await.unwrap_or(false))
    }

    /// Let the plugin's copy of the Mac's clipboard catch up with the clipboard now: a change is
    /// sent to the plugin as an event, for a paste there to read.
    ///
    /// TEMPORARY(clipboard): a workaround, to be removed when embedded_gpui is fixed. Its surface
    /// does this at a ⌘ key press (`host/surface.rs`, `key_down`), but only queues the event,
    /// which is sent after the key's query has gone out, so the first paste after something was
    /// copied elsewhere read the old copy. Doing it ahead (when a window of the app gets the
    /// keyboard, or is clicked) puts the change before the key. Remove this and its calls
    /// (`grep -rn "TEMPORARY(clipboard)"`) once embedded_gpui delivers the change before the key.
    pub fn refresh_clipboard(&self, cx: &mut App) {
        let clipboard = self.host.read(cx).clipboard();
        clipboard.update(cx, |clipboard, cx| clipboard.refresh(cx));
    }

    /// Call `changed` whenever the plugin says its settings sections changed
    /// (`settings_changed` in the plugin API): the app asks for them again.
    pub fn observe_settings(&self, cx: &mut App, changed: impl Fn(&mut App) + 'static) -> Subscription {
        self.root.observe(cx, changed)
    }

    /// Make a call on the plugin's root, unless it stopped, and wait for the answer
    /// for at most [`CALL_TIMEOUT`]. A timeout or embedded_gpui's stop marks it
    /// stopped; the plugin's own errors (such as an unknown operation) don't.
    fn call<T: 'static, R: Future<Output = Result<T>> + 'static>(
        &self,
        make: impl FnOnce(&Remote<PluginApi>, &mut App) -> R,
        cx: &mut App,
    ) -> Task<Result<T>> {
        let name = self.manifest.plugin.name.clone();
        if let Some(reason) = self.stopped() {
            return Task::ready(Err(anyhow!("{name} stopped: {reason}")));
        }
        let answer = make(&self.root, cx);
        let timeout = cx.background_executor().timer(CALL_TIMEOUT);
        let stopped = self.stopped.clone();
        cx.spawn(async move |_| {
            let (result, stops) = match select(pin!(answer), timeout).await {
                Either::Left((result, _)) => {
                    let stops = result.as_ref().is_err_and(|error| {
                        error.to_string().starts_with(STOPPED_BY_EMBEDDED_GPUI)
                    });
                    (result, stops)
                }
                Either::Right(_) => (Err(anyhow!("it didn't answer within {CALL_TIMEOUT:?}")), true),
            };
            if stops && let Err(error) = &result {
                stopped.borrow_mut().get_or_insert_with(|| format!("{error:#}"));
                log::error!("{name} stopped: {error:#}");
            }
            result
        })
    }
}

/// Ask every plugin what fits `input`, all at once, and rank the answers (see
/// [`rank`]); a candidate's `plugin` is its index in `plugins`. A stopped plugin, or
/// one that fails, contributes nothing. Blank input has no candidates and calls no
/// plugin.
pub fn detect_all(plugins: &[Plugin], input: &Input, cx: &mut App) -> Task<Vec<Candidate>> {
    if input.text.trim().is_empty() {
        return Task::ready(Vec::new());
    }
    let answers: Vec<_> = plugins.iter().map(|plugin| plugin.detect(input, cx)).collect();
    let manifests: Vec<Rc<Manifest>> = plugins.iter().map(|plugin| plugin.manifest.clone()).collect();
    cx.spawn(async move |_| {
        let detections: Vec<Vec<Detection>> = join_all(answers)
            .await
            .into_iter()
            .map(|answer| answer.unwrap_or_default())
            .collect();
        rank(
            manifests
                .iter()
                .map(|manifest| &**manifest)
                .zip(detections.iter().map(Vec::as_slice)),
        )
    })
}
