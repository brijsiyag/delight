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
use delight_protocol::{Detection, HostApi, Input, PluginApi, PluginApiCaller as _, ToolApi};
use embedded_gpui::gpui::{App, Entity, PlatformTextSystem, Task};
use embedded_gpui::{PluginHost, PluginHostHandle as _, PluginOptions, Remote, Shared, Surface};
use futures::future::{Either, join_all, select};
use wasmtime_wasi::{DirPerms, FilePerms};

use crate::{Candidate, Granted, rank};

/// Longest the app waits for a plugin's answer. A plugin that stops mid-call fails
/// the call, but one called just after it stopped never answers; the timeout makes
/// that a stop too.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(3);

/// How embedded_gpui's calls fail when the plugin stopped (a trap: its turn took too
/// long, it ran out of memory, or it panicked): every failed call reads "call failed:
/// {reason}", and a stop's reason starts "plugin stopped: ".
const STOPPED_BY_EMBEDDED_GPUI: &str = "call failed: plugin stopped";

/// The sandbox a plugin runs in, from what its manifest grants: its data folder
/// (created if needed) at `/data`, and the network (sockets and name lookups) only
/// with [`Permission::Network`]. Nothing else outside the sandbox is reachable.
pub fn plugin_options(
    manifest: &Manifest,
    data_dir: PathBuf,
    text_system: Arc<dyn PlatformTextSystem>,
) -> PluginOptions {
    let network = manifest.plugin.permission::<NetworkPermission>().is_some();
    PluginOptions::new(text_system).with_wasi(move |wasi| {
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
/// The first call that finds it stopped (see [`CALL_TIMEOUT`]) marks it stopped, with
/// that reason; it isn't called again, and every call fails at once.
#[derive(Clone)]
pub struct Plugin {
    manifest: Rc<Manifest>,
    host: Entity<PluginHost>,
    root: Remote<PluginApi>,
    stopped: Rc<RefCell<Option<String>>>,
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
                Plugin {
                    manifest: Rc::new(manifest),
                    host,
                    root,
                    stopped: Rc::default(),
                }
            }))
        })
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
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

    /// Ask the plugin to draw its settings page on `surface`, shared with it like a
    /// tool's: whether it has one. A plugin that can't answer (built before settings
    /// pages, or stopped) has none.
    pub fn open_settings(&self, surface: &Entity<Surface>, cx: &mut App) -> Task<bool> {
        let host = self.host.clone();
        let surface = surface.clone();
        let asked = self.call(
            move |root, cx| {
                let surface = host.share(&surface, cx);
                root.open_settings(surface, cx)
            },
            cx,
        );
        cx.spawn(async move |_| asked.await.unwrap_or(false))
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
