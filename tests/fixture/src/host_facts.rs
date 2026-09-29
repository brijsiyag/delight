//! The fixture's actions for what the app knows for a plugin, which the headless tests
//! perform: secrets, the UTC offset, and the settings page. Each
//! toasts what it found.

use delight_plugin_api::gpui::Context;
use delight_plugin_api::host;

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
