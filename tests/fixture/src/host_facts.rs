//! The fixture's actions for what the app knows for a plugin, which the headless tests
//! perform: secrets, the UTC offset, and the settings page. Each
//! toasts what it found.

use delight_plugin_api::gpui::Context;
use delight_plugin_api::host;
use serde::{Deserialize, Serialize};

use super::Echo;

/// Save the input as the secret "token", read it back, delete it, read it again, and
/// toast "{saved:?} {deleted:?}".
pub fn secrets(input: &str, cx: &mut Context<Echo>) {
    let host = host(cx);
    let value = input.to_string();
    cx.spawn(async move |_, cx| {
        let saved = cx.update(|cx| host.set_secret("token", value, cx)).await;
        let read = cx.update(|cx| host.secret("token", cx)).await;
        let deleted = cx.update(|cx| host.set_secret("token", "", cx)).await;
        let gone = cx.update(|cx| host.secret("token", cx)).await;
        let message = match (saved, read, deleted, gone) {
            (Ok(()), Ok(read), Ok(()), Ok(gone)) => format!("{read:?} {gone:?}"),
            _ => "failed".to_string(),
        };
        cx.update(|cx| host.toast(message, cx));
    })
    .detach();
}

/// Toast "offset {seconds}".
pub fn facts(cx: &mut Context<Echo>) {
    let offset = host(cx).utc_offset_seconds(cx);
    host(cx).toast(format!("offset {offset}"), cx);
}

/// The type the fixture's settings are read into.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct Saved {
    account: Option<String>,
    range: u32,
}

/// Save `Saved { account: input, range: 7 }`, read it back, clear it, read again, and
/// toast "{saved:?} {cleared:?}".
pub fn settings(input: &str, cx: &mut Context<Echo>) {
    let host = host(cx);
    let account = Some(input.to_string());
    cx.spawn(async move |_, cx| {
        let saved = cx.update(|cx| host.set_settings(&Saved { account, range: 7 }, cx)).await;
        let read = cx.update(|cx| host.settings::<Saved>(cx)).await;
        let cleared = cx.update(|cx| host.clear_settings(cx)).await;
        let gone = cx.update(|cx| host.settings::<Saved>(cx)).await;
        let message = match (saved, read, cleared, gone) {
            (Ok(()), Ok(read), Ok(()), Ok(gone)) => format!("{read:?} {gone:?}"),
            _ => "failed".to_string(),
        };
        cx.update(|cx| host.toast(message, cx));
    })
    .detach();
}

/// Read the settings as `Saved` and toast "{settings:?}" or why they can't be read.
pub fn read_settings(cx: &mut Context<Echo>) {
    let host = host(cx);
    let read = host.settings::<Saved>(cx);
    cx.spawn(async move |_, cx| {
        let message = match read.await {
            Ok(saved) => format!("{saved:?}"),
            Err(error) => format!("{error:#}"),
        };
        cx.update(|cx| host.toast(message, cx));
    })
    .detach();
}
