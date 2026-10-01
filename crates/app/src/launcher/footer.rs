//! The footer's actions and the keys that run them: each action's own shortcut, in
//! the tool's order. Only ↵, ⌘↵ and ⌥1 to ⌥9 are keys an action may have; any other
//! keystroke a plugin sends, a shortcut the keymap already binds where the focus is (the
//! keymap wins), or an earlier action's, is dropped: that action is only clicked. A hidden
//! action has its key and no button.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use delight_protocol::{Action, Shortcut};
use gpui::Keystroke;

/// Say once, however often the footer is worked out (every redraw), that an action has a key it
/// can't have.
fn warn_once(action: &str, keystroke: &str) {
    static SEEN: OnceLock<Mutex<HashSet<(String, String)>>> = OnceLock::new();
    let seen = SEEN.get_or_init(Default::default);
    if seen.lock().is_ok_and(|mut seen| seen.insert((action.to_string(), keystroke.to_string()))) {
        log::warn!("action {action:?} has {keystroke:?}, which isn't one of ↵, ⌘↵ and ⌥1 to ⌥9: it is only clicked");
    }
}

/// The actions, in order, each with the key that runs it (`None`: clicked).
pub fn keyed(actions: Vec<Action>, is_bound: impl Fn(&Keystroke) -> bool) -> Vec<(Action, Option<Keystroke>)> {
    let mut taken: Vec<Keystroke> = Vec::new();
    actions
        .into_iter()
        .map(|action| {
            let key = match &action.shortcut {
                Shortcut::Keystroke(keystroke) if Shortcut::is_allowed(keystroke) => Keystroke::parse(keystroke).ok(),
                // Not one of the keys every tool has: only clicked, like `ClickOnly`.
                Shortcut::Keystroke(keystroke) => {
                    warn_once(&action.id, keystroke);
                    None
                }
                Shortcut::ClickOnly => None,
            }
            .filter(|key| !is_bound(key) && !taken.iter().any(|taken| matches(taken, key)));
            taken.extend(key.clone());
            (action, key)
        })
        .collect()
}

/// The actions that get a button in the footer: in the tool's order, skipping hidden ones, at most
/// `max` of them (the footer has room for `FOOTER_ACTIONS`, 4). The others still answer their keys:
/// keys are matched against every action, with or without a button.
pub fn buttons(keyed: Vec<(Action, Option<Keystroke>)>, max: usize) -> Vec<(Action, Option<Keystroke>)> {
    keyed.into_iter().filter(|(action, _)| !action.hidden).take(max).collect()
}

/// Whether `pressed` is `own` (same key, same modifiers).
pub fn matches(own: &Keystroke, pressed: &Keystroke) -> bool {
    own.modifiers == pressed.modifiers && own.key.eq_ignore_ascii_case(&pressed.key)
}

#[cfg(test)]
mod tests {
    use delight_protocol::ActionStyle;

    use super::*;

    fn action(id: &str, shortcut: Option<&str>) -> Action {
        Action {
            id: id.into(),
            label: id.into(),
            shortcut: shortcut.map_or(Shortcut::ClickOnly, |key| Shortcut::Keystroke(key.into())),
            style: ActionStyle::Normal,
            hidden: false,
        }
    }

    fn hidden(id: &str, shortcut: &str) -> Action {
        Action { hidden: true, ..action(id, Some(shortcut)) }
    }

    fn keys(actions: Vec<Action>, bound: &[&str]) -> Vec<(String, Option<Keystroke>)> {
        let bound: Vec<Keystroke> = bound.iter().map(|key| Keystroke::parse(key).unwrap()).collect();
        let is_bound = |key: &Keystroke| bound.iter().any(|bound| matches(bound, key));
        keyed(actions, is_bound).into_iter().map(|(action, key)| (action.id, key)).collect()
    }

    fn key(keystroke: &str) -> Option<Keystroke> {
        Some(Keystroke::parse(keystroke).unwrap())
    }

    #[test]
    fn keeps_the_order_and_own_keys() {
        let actions = vec![action("copy", Some("enter")), action("open", None), action("delete", Some("alt-1"))];
        let expected = [("copy".to_string(), key("enter")), ("open".to_string(), None), ("delete".to_string(), key("alt-1"))];
        assert_eq!(keys(actions, &[]), expected);
    }

    #[test]
    fn only_enter_cmd_enter_and_option_1_to_9_are_keys() {
        let all = ["enter", "cmd-enter", "alt-1", "alt-2", "alt-3", "alt-4", "alt-5", "alt-6", "alt-7", "alt-8", "alt-9"];
        let actions: Vec<Action> = all.iter().map(|keys| action(keys, Some(keys))).collect();
        assert!(keys(actions, &[]).iter().all(|(_, key)| key.is_some()), "every allowed key works");
        let refused = ["cmd-k", "cmd-shift-enter", "shift-enter", "cmd-1", "alt-0", "alt-10", "ctrl-enter", "f5", "a"];
        let actions: Vec<Action> = refused.iter().map(|keys| action(keys, Some(keys))).collect();
        assert!(keys(actions, &[]).iter().all(|(_, key)| key.is_none()), "any other key is only clicked");
    }

    #[test]
    fn the_keymap_and_earlier_actions_win() {
        let actions = vec![action("a", Some("cmd-enter")), action("b", Some("enter")), action("c", Some("enter"))];
        let expected = [("a".to_string(), None), ("b".to_string(), key("enter")), ("c".to_string(), None)];
        assert_eq!(keys(actions, &["cmd-enter"]), expected);
    }

    #[test]
    fn a_hidden_action_keeps_its_key_but_takes_no_button() {
        let actions = vec![
            action("repo", Some("enter")),
            hidden("prd", "alt-1"),
            action("argo", Some("cmd-enter")),
            hidden("int", "alt-2"),
            action("ringmaster", Some("alt-4")),
            action("logs", None),
            action("vault", None),
        ];
        let keyed = keyed(actions, |_| false);
        assert_eq!((keyed[1].0.id.as_str(), keyed[1].1.clone()), ("prd", key("alt-1")), "its key works");
        let shown: Vec<String> = buttons(keyed, 4).into_iter().map(|(action, _)| action.id).collect();
        assert_eq!(shown, ["repo", "argo", "ringmaster", "logs"], "four buttons, none of them hidden");
    }

    #[test]
    fn own_shortcuts_match_exactly() {
        let own = Keystroke::parse("cmd-shift-enter").unwrap();
        assert!(matches(&own, &Keystroke::parse("cmd-shift-enter").unwrap()));
        assert!(!matches(&own, &Keystroke::parse("enter").unwrap()), "↵ alone isn't ⌘⇧↵");
    }
}
