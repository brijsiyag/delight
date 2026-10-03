//! Plugin updates (`docs/plan.md`, step 16), each plugin on its own. At launch and once a day, and
//! when its page's Check Now is clicked, an installed plugin that names where it is published has
//! its manifest there read, and a newer version is offered on its page. An update that asks for
//! nothing new replaces the plugin's file and restarts that plugin alone; one that asks for more
//! goes through the install window, like a new plugin. A plugin set to update automatically (its
//! page's switch, on unless turned off) installs the first kind on its own while it isn't in use.
//! Reading a manifest, downloading a file and checking it is `delight_runtime::updates`; built-ins
//! update with the app.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use delight_protocol::Manifest;
use delight_runtime::updates::{self, Release, is_newer};
use futures::StreamExt as _;
use gpui::{App, AppContext as _, Global, Task};

use super::{all, install, save_download, sources};
use crate::{install_window, launcher, permissions, plugin_windows, settings};

/// The first look waits for the plugins to have started, and for launch to be over.
const FIRST_CHECK: Duration = Duration::from_secs(30);
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// How long what a look found ("It is up to date") shows in place of when the last look was.
pub const ANSWER_SHOWN: Duration = Duration::from_secs(5);

/// What the last look at a plugin's location found, when no newer version is offered.
#[derive(Clone, Debug, PartialEq)]
pub enum Check {
    Checking,
    /// The version there is the one installed.
    UpToDate,
    /// Why its manifest there couldn't be read.
    Failed(String),
}

/// A newer version of an installed plugin, and where installing it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub release: Release,
    pub state: State,
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// Waiting for Update (or, to install on its own, for the plugin to be out of use).
    Ready,
    /// How many bytes of it have come, and how many there are once the server says.
    Downloading { done: u64, total: Option<u64> },
    /// Found while installing on its own: it asks for new permissions, so it waits for Update.
    AsksForMore,
    /// Why the last try didn't install it.
    Failed(String),
}

/// By plugin id.
#[derive(Default)]
struct Updates {
    checks: HashMap<String, Check>,
    /// When a look last read what is published (a failed look leaves it as it was).
    looked: HashMap<String, SystemTime>,
    offers: HashMap<String, Offer>,
    _schedule: Option<Task<()>>,
}

impl Global for Updates {}

/// Look at every installed plugin's location a little after launch, then once a day.
pub fn start(cx: &mut App) {
    let schedule = cx.spawn(async move |cx| {
        cx.background_executor().timer(FIRST_CHECK).await;
        loop {
            cx.update(|cx| {
                for (manifest, _) in installed(cx) {
                    check(&manifest.plugin.id, cx);
                }
            });
            cx.background_executor().timer(EVERY).await;
        }
    });
    cx.set_global(Updates { _schedule: Some(schedule), ..Updates::default() });
}

/// What the last look at this plugin's location found, if it was looked at.
pub fn last_check(plugin_id: &str, cx: &App) -> Option<Check> {
    cx.try_global::<Updates>()?.checks.get(plugin_id).cloned()
}

/// When a look at this plugin's location last read what is published there, since Delight started.
pub fn last_looked(plugin_id: &str, cx: &App) -> Option<SystemTime> {
    cx.try_global::<Updates>()?.looked.get(plugin_id).copied()
}

/// The update offered for this plugin, while it is newer than the one running.
pub fn offer(plugin_id: &str, cx: &App) -> Option<Offer> {
    let offer = cx.try_global::<Updates>()?.offers.get(plugin_id)?;
    let running = all(cx).iter().find(|plugin| plugin.manifest().plugin.id == plugin_id)?.manifest().plugin.version.clone();
    is_newer(&offer.release.version, &running).then(|| offer.clone())
}

/// Read this plugin's manifest where it is published, in the background; then offer a newer
/// version, and install it if it may install on its own.
pub fn check(plugin_id: &str, cx: &mut App) {
    let Some((manifest, location)) = installed(cx).into_iter().find(|(manifest, _)| manifest.plugin.id == plugin_id) else {
        return;
    };
    if !cx.has_global::<Updates>() {
        return;
    }
    let updates = cx.global_mut::<Updates>();
    if updates.checks.get(plugin_id) == Some(&Check::Checking) {
        return;
    }
    updates.checks.insert(plugin_id.to_string(), Check::Checking);
    cx.refresh_windows();
    cx.spawn(async move |cx| {
        let id = manifest.plugin.id.clone();
        let release = cx.background_spawn(async move { updates::fetch_release(&location, &id) }).await;
        cx.update(|cx| checked(&manifest, release, cx));
    })
    .detach();
}

/// Install the update offered for this plugin: the user asked.
pub fn update(plugin_id: &str, cx: &mut App) {
    install_update(plugin_id, false, cx);
}

/// The plugin's manifest was read (or why not): offer a newer version, keeping how far the same
/// version already offered got, and install it if it may install on its own.
fn checked(installed: &Manifest, release: Result<Release>, cx: &mut App) {
    let id = installed.plugin.id.clone();
    let updates = cx.global_mut::<Updates>();
    let ready = match release {
        Err(error) => {
            log::warn!("looking for an update of {id}: {error:#}");
            updates.checks.insert(id.clone(), Check::Failed(format!("{error:#}")));
            false
        }
        Ok(release) if release.newer_than(&installed.plugin) => {
            updates.checks.remove(&id);
            updates.looked.insert(id.clone(), SystemTime::now());
            // The same version keeps its state; a failed one is tried again.
            let state = match updates.offers.get(&id).filter(|offer| offer.release == release).map(|offer| &offer.state) {
                Some(downloading @ State::Downloading { .. }) => downloading.clone(),
                Some(State::AsksForMore) => State::AsksForMore,
                _ => State::Ready,
            };
            let ready = state == State::Ready;
            updates.offers.insert(id.clone(), Offer { release, state });
            ready
        }
        Ok(_) => {
            updates.checks.insert(id.clone(), Check::UpToDate);
            updates.looked.insert(id.clone(), SystemTime::now());
            updates.offers.remove(&id);
            // Its page says so for a moment, then when it looked.
            cx.spawn(async move |cx| {
                cx.background_executor().timer(ANSWER_SHOWN).await;
                cx.update(|cx| cx.refresh_windows());
            })
            .detach();
            false
        }
    };
    cx.refresh_windows();
    if ready && settings::get(cx).updates_automatically(&id) && !in_use(&id, cx) {
        install_update(&id, true, cx);
    }
}

/// Download the offered update and check it, then replace the plugin, or open the install window
/// when it asks for more. `automatic`: on its own, so one that asks for more waits for the user,
/// and one whose plugin came into use meanwhile waits for the next check.
fn install_update(plugin_id: &str, automatic: bool, cx: &mut App) {
    let Some((installed, location)) = installed(cx).into_iter().find(|(manifest, _)| manifest.plugin.id == plugin_id) else {
        return;
    };
    let Some(offer) = cx.global_mut::<Updates>().offers.get_mut(plugin_id) else { return };
    if matches!(offer.state, State::Downloading { .. }) {
        return;
    }
    offer.state = State::Downloading { done: 0, total: None };
    let release = offer.release.clone();
    cx.refresh_windows();
    let id = plugin_id.to_string();
    cx.spawn(async move |cx| {
        let (sender, mut moved) = futures::channel::mpsc::unbounded();
        let old = installed.clone();
        let _worker = cx.background_spawn(async move {
            let progress = sender.clone();
            let downloaded = updates::download_with_progress(&location, &release, Some(&old), move |done, total| {
                progress.unbounded_send(Moved::Progress(done, total)).ok();
            })
            .and_then(|downloaded| Ok((save_download(&downloaded)?, downloaded.manifest)));
            sender.unbounded_send(Moved::Finished(Box::new(downloaded))).ok();
        });
        while let Some(message) = moved.next().await {
            match message {
                Moved::Progress(done, total) => cx.update(|cx| {
                    if let Some(offer) = cx.global_mut::<Updates>().offers.get_mut(&id) {
                        offer.state = State::Downloading { done, total };
                    }
                    cx.refresh_windows();
                }),
                Moved::Finished(downloaded) => {
                    let downloaded = (*downloaded).map(|(file, manifest)| (manifest, file));
                    cx.update(|cx| downloaded_update(&id, &installed, automatic, downloaded, cx));
                    break;
                }
            }
        }
    })
    .detach();
}

/// What a download says while it runs: bytes so far (and in all, when known), then the file kept
/// in Delight's caches with its manifest, or why not.
enum Moved {
    Progress(u64, Option<u64>),
    Finished(Box<Result<(PathBuf, Manifest)>>),
}

/// The update's file is here, checked (or why not): install it, or hand it to the user.
fn downloaded_update(id: &str, installed: &Manifest, automatic: bool, downloaded: Result<(Manifest, PathBuf)>, cx: &mut App) {
    let state = match downloaded {
        Err(error) => {
            log::warn!("updating {id}: {error:#}");
            State::Failed(format!("{error:#}"))
        }
        Ok((manifest, _)) if automatic && permissions::asks_for_more(installed, &manifest) => State::AsksForMore,
        Ok((manifest, file)) if permissions::asks_for_more(installed, &manifest) => {
            install_window::open_files(vec![file], cx);
            State::Ready
        }
        Ok(_) if automatic && in_use(id, cx) => State::Ready,
        Ok((manifest, file)) => match install(&file, &manifest, cx) {
            Ok(()) => {
                std::fs::remove_file(&file).ok();
                log::info!("updated {id} from {} to {}", installed.plugin.version, manifest.plugin.version);
                cx.global_mut::<Updates>().offers.remove(id);
                return;
            }
            Err(error) => State::Failed(format!("{error:#}")),
        },
    };
    if let Some(offer) = cx.global_mut::<Updates>().offers.get_mut(id) {
        offer.state = state;
    }
    cx.refresh_windows();
}

/// The installed plugins (not the built-ins) that name where they are published, with that
/// location.
fn installed(cx: &App) -> Vec<(Manifest, String)> {
    all(cx)
        .iter()
        .zip(sources(cx).iter())
        .filter(|(_, source)| !source.built_in)
        .filter_map(|(plugin, _)| {
            let manifest = plugin.manifest();
            Some((manifest.clone(), manifest.plugin.update.clone()?))
        })
        .collect()
}

/// A tool of the plugin is shown, or one of its windows is open: restarting it now would take
/// that away.
fn in_use(plugin_id: &str, cx: &App) -> bool {
    launcher::shows_plugin(plugin_id, cx) || plugin_windows::has_open(plugin_id, cx)
}
