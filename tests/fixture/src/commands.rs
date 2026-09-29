//! The fixture's commands action, which the headless tests perform: run the program on
//! the input's first line, with the following lines as its arguments, and toast
//! "{status}|{stdout}|{stderr}" or why it couldn't.

use delight_plugin_api::gpui::Context;
use delight_plugin_api::{Command, host};

use super::Echo;

pub fn run(input: &str, cx: &mut Context<Echo>) {
    let mut lines = input.lines();
    let command = Command {
        program: lines.next().unwrap_or_default().to_string(),
        args: lines.map(str::to_string).collect(),
        ..Command::default()
    };
    let ran = host(cx).run(command, cx);
    cx.spawn(async move |_, cx| {
        let message = match ran.await {
            Ok(output) => format!("{:?}|{}|{}", output.code, output.stdout, output.stderr),
            Err(error) => format!("{error:#}"),
        };
        cx.update(|cx| host(cx).toast(message, cx));
    })
    .detach();
}
