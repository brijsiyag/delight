//! The input's placeholder: a tip, a different one, picked at random, each time the
//! launcher shows. Delight's own are about its keys; plugins give theirs (at most 5
//! each, in their manifests), shown while they have a tool on.

use std::time::{SystemTime, UNIX_EPOCH};

use delight_ui::keystroke_label;
use gpui::App;

use crate::{hotkey, plugins, settings};

/// Tips that hold whatever the settings are.
const TIPS: &[&str] = &[
    "⌘K clears the input",
    "⌘, opens Settings",
    "⌘1 to ⌘9 pick a tool from the list",
    "↓ at the end of the input moves into the tool list",
];

/// Tips about the input history, while it's on.
const HISTORY_TIPS: &[&str] = &[
    "⌃R searches what you typed before",
    "Tab takes the grey completion",
    "⌃N and ⌃P step through older and newer completions",
];

/// A tip other than `previous`, picked at random: Delight's own, or a plugin's.
pub fn next(previous: &str, cx: &App) -> String {
    let mut tips: Vec<String> = TIPS.iter().map(|tip| tip.to_string()).collect();
    if settings::get(cx).input_history {
        tips.extend(HISTORY_TIPS.iter().map(|tip| tip.to_string()));
    }
    if let Some(shortcut) = hotkey::current(cx) {
        tips.push(format!("{} shows Delight from any app", keystroke_label(&shortcut)));
    }
    tips.extend(plugin_tips(cx));
    let random = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |time| time.subsec_nanos());
    pick(tips, previous, random as usize)
}

/// The tips of the plugins that have a tool on, as their authors wrote them.
fn plugin_tips(cx: &App) -> Vec<String> {
    let settings = settings::get(cx);
    plugins::all(cx)
        .iter()
        .map(|plugin| plugin.manifest())
        .filter(|manifest| {
            let id = &manifest.plugin.id;
            manifest.operations.iter().any(|operation| settings.tool_runs(id, &operation.id))
        })
        .flat_map(|manifest| manifest.plugin.tips.iter().cloned())
        .collect()
}

/// The `random`th of `tips`, leaving out `previous`.
fn pick(mut tips: Vec<String>, previous: &str, random: usize) -> String {
    tips.retain(|tip| tip != previous);
    tips.swap_remove(random % tips.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_the_same_tip_twice_in_a_row() {
        let tips = || TIPS.iter().map(|tip| tip.to_string()).collect::<Vec<_>>();
        for random in 0..20 {
            assert_ne!(pick(tips(), TIPS[0], random), TIPS[0]);
        }
    }
}
