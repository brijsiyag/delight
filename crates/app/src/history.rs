//! The launcher's input history, in `input-history.json`:
//!
//! * **Remembered inputs**: inputs *plugins* chose to remember
//!   (`HostApi::remember_input`), each with the tool it was for (plugin and
//!   operation). A log search remembers its queries; a JSON formatter remembers
//!   nothing. While typing, the launcher shows how a remembered input would
//!   complete the text, and taking it brings that tool up.
//! * **The input to restore**: the launcher's input when it last hid, brought back
//!   at launch.
//!
//! It's kept apart from other settings because inputs often hold tokens.

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Context as _;
use gpui::{App, Global};
use serde::{Deserialize, Serialize};

use crate::files::{read_json, write_json};

/// Remembered inputs kept; the oldest go first.
const MAX_REMEMBERED_INPUTS: usize = 10_000;
/// Inputs with more characters than this (big pastes) are neither remembered nor
/// restored.
const MAX_INPUT_CHARS: usize = 1729;
/// Completions, and a history search with no query, look only at this many of the
/// newest inputs; a search with a query looks at all of them.
const RECENT_INPUTS: usize = 200;
/// Most inputs a history search returns.
const MAX_SEARCH_RESULTS: usize = 200;
/// All remembered text stays under this, oldest dropped first, so the file stays
/// quick to load and rewrite however long the inputs are.
const MAX_TOTAL_TEXT_BYTES: usize = 4 * 1024 * 1024;

/// An input a plugin chose to remember, and the tool it was for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RememberedInput {
    pub plugin_id: String,
    /// Empty in inputs the previous attempt remembered before it recorded
    /// operations.
    #[serde(default)]
    pub operation_id: String,
    pub text: String,
}

/// How a remembered input would complete what's typed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Completion<'a> {
    /// The rest of the remembered input, after what's typed.
    pub remainder: &'a str,
    /// The tool it was remembered for.
    pub plugin_id: &'a str,
    pub operation_id: &'a str,
}

/// The file's contents.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct HistoryFile {
    input_to_restore: String,
    /// Most recent last.
    remembered: Vec<RememberedInput>,
}

pub struct InputHistory {
    path: PathBuf,
    file: HistoryFile,
}

impl Global for InputHistory {}

/// Load the history from Delight's folder.
pub fn init(cx: &mut App) {
    cx.set_global(InputHistory::open(crate::app_dir().join("input-history.json")));
}

pub fn get(cx: &App) -> &InputHistory {
    cx.global::<InputHistory>()
}

pub fn get_mut(cx: &mut App) -> &mut InputHistory {
    cx.global_mut::<InputHistory>()
}

/// Forget everything: what tools remembered, and the input to restore. The file goes
/// too.
pub fn erase(cx: &mut App) {
    let history = get_mut(cx);
    history.file = HistoryFile::default();
    match std::fs::remove_file(&history.path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            log::error!("erasing the input history: {error}");
        }
        _ => {}
    }
}

/// Forget what a deleted plugin remembered.
pub fn forget_plugin(plugin_id: &str, cx: &mut App) {
    let history = get_mut(cx);
    let before = history.file.remembered.len();
    history.file.remembered.retain(|input| input.plugin_id != plugin_id);
    if history.file.remembered.len() != before
        && let Err(error) = history.save()
    {
        log::error!("forgetting {plugin_id}'s inputs: {error:#}");
    }
}

fn too_long(text: &str) -> bool {
    text.chars().count() > MAX_INPUT_CHARS
}

impl InputHistory {
    /// The history stored at `path`; empty if there's none yet, or it can't be read.
    fn open(path: PathBuf) -> Self {
        let file = read_json(&path).unwrap_or_default();
        Self { path, file }
    }

    /// The input to bring back at launch.
    pub fn input_to_restore(&self) -> Option<&str> {
        Some(self.file.input_to_restore.as_str()).filter(|input| !input.is_empty())
    }

    /// Save the launcher's input to bring back at the next launch. An input that's
    /// too long isn't kept (nor is the one before).
    pub fn set_input_to_restore(&mut self, input: &str) -> anyhow::Result<()> {
        let input = if too_long(input) { "" } else { input };
        if input == self.file.input_to_restore {
            return Ok(());
        }
        self.file.input_to_restore = input.to_string();
        self.save()
    }

    /// How the `nth` remembered input starting with `typed` would complete it: 0 is
    /// the newest, 1 the one before (each text counted once), among the
    /// [`RECENT_INPUTS`] newest. `None` when there's no such input.
    pub fn completion_for(&self, typed: &str, nth: usize) -> Option<Completion<'_>> {
        // Nothing typed completes with any input; only spaces, with none.
        if !typed.is_empty() && typed.trim().is_empty() {
            return None;
        }
        let mut seen = HashSet::new();
        self.file
            .remembered
            .iter()
            .rev()
            .take(RECENT_INPUTS)
            .filter(|input| input.text.len() > typed.len() && input.text.starts_with(typed))
            .filter(|input| seen.insert(input.text.as_str()))
            .nth(nth)
            .map(|input| Completion {
                remainder: &input.text[typed.len()..],
                plugin_id: &input.plugin_id,
                operation_id: &input.operation_id,
            })
    }

    /// The remembered inputs containing every word of `query` (in any case), newest
    /// first, from all of them; the [`RECENT_INPUTS`] newest for a blank query.
    pub fn search(&self, query: &str) -> Vec<&RememberedInput> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return self.file.remembered.iter().rev().take(RECENT_INPUTS).collect();
        }
        self.file
            .remembered
            .iter()
            .rev()
            .filter(|input| {
                let text = input.text.to_lowercase();
                words.iter().all(|word| text.contains(word.as_str()))
            })
            .take(MAX_SEARCH_RESULTS)
            .collect()
    }

    /// Remember `text` for a plugin's operation as the newest input, and save. The
    /// same tool's older inputs that are just its beginning (partial inputs like
    /// `con` before `config-manager`) are dropped, so they don't crowd out the full
    /// one. Blank and too-long inputs aren't remembered.
    pub fn remember(&mut self, plugin_id: &str, operation_id: &str, text: &str) -> anyhow::Result<()> {
        if text.trim().is_empty() || too_long(text) {
            return Ok(());
        }
        let remembered = &mut self.file.remembered;
        let same_tool = |input: &RememberedInput| input.plugin_id == plugin_id && input.operation_id == operation_id;
        if remembered.last().is_some_and(|input| same_tool(input) && input.text == text) {
            return Ok(());
        }
        remembered.retain(|input| !same_tool(input) || !text.starts_with(input.text.as_str()));
        remembered.push(RememberedInput {
            plugin_id: plugin_id.to_string(),
            operation_id: operation_id.to_string(),
            text: text.to_string(),
        });
        let mut oldest_kept = remembered.len().saturating_sub(MAX_REMEMBERED_INPUTS);
        let mut total: usize = remembered[oldest_kept..].iter().map(|input| input.text.len()).sum();
        while total > MAX_TOTAL_TEXT_BYTES {
            total -= remembered[oldest_kept].text.len();
            oldest_kept += 1;
        }
        remembered.drain(..oldest_kept);
        self.save()
    }

    fn save(&self) -> anyhow::Result<()> {
        write_json(&self.path, &self.file).with_context(|| format!("saving {}", self.path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOGS: &str = "acme.logs";
    const AB: &str = "acme.ab";
    const SEARCH: &str = "search";

    /// A history in a fresh temporary file, removed again when the test ends.
    struct TempHistory(InputHistory);

    impl TempHistory {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("delight-history-{name}-{}.json", std::process::id()));
            let _ = std::fs::remove_file(&path);
            Self(InputHistory::open(path))
        }

        fn reopened(&self) -> InputHistory {
            InputHistory::open(self.0.path.clone())
        }

        fn texts(&self) -> Vec<&str> {
            self.0.file.remembered.iter().map(|input| input.text.as_str()).collect()
        }
    }

    impl Drop for TempHistory {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0.path);
        }
    }

    impl std::ops::Deref for TempHistory {
        type Target = InputHistory;
        fn deref(&self) -> &InputHistory {
            &self.0
        }
    }

    impl std::ops::DerefMut for TempHistory {
        fn deref_mut(&mut self) -> &mut InputHistory {
            &mut self.0
        }
    }

    fn input(text: impl Into<String>) -> RememberedInput {
        RememberedInput { plugin_id: LOGS.into(), operation_id: SEARCH.into(), text: text.into() }
    }

    #[test]
    fn completes_with_the_most_recent_match_and_its_tool() {
        let mut h = TempHistory::new("complete");
        h.remember(LOGS, SEARCH, r#"application_name: "billing""#).unwrap();
        h.remember(LOGS, SEARCH, r#"application_name: "config-manager""#).unwrap();
        h.remember(AB, SEARCH, "apple").unwrap();
        assert_eq!(
            h.completion_for("application", 0),
            Some(Completion { remainder: r#"_name: "config-manager""#, plugin_id: LOGS, operation_id: SEARCH })
        );
        let newest = Some(Completion { remainder: "le", plugin_id: AB, operation_id: SEARCH });
        assert_eq!(h.completion_for("app", 0), newest, "most recent first");
        assert_eq!(h.completion_for("apple", 0), None, "nothing longer");
        assert_eq!(h.completion_for("  ", 0), None);
    }

    #[test]
    fn steps_through_older_completions_once_each() {
        let mut h = TempHistory::new("older");
        for text in ["git status", "git log", "git status", "grep x"] {
            h.remember(LOGS, SEARCH, text).unwrap();
        }
        let remainder = |nth| h.completion_for("git ", nth).map(|c| c.remainder);
        assert_eq!([remainder(0), remainder(1), remainder(2)], [Some("status"), Some("log"), None]);
    }

    #[test]
    fn an_empty_input_completes_with_any_input_but_spaces_with_none() {
        let mut h = TempHistory::new("empty");
        for text in ["git status", "git log", "git status"] {
            h.remember(LOGS, SEARCH, text).unwrap();
        }
        let whole = |nth| h.completion_for("", nth).map(|c| c.remainder);
        assert_eq!([whole(0), whole(1), whole(2)], [Some("git status"), Some("git log"), None]);
        assert!(h.completion_for("  ", 0).is_none());
    }

    #[test]
    fn searches_by_words_newest_first() {
        let mut h = TempHistory::new("search");
        h.remember(LOGS, SEARCH, "level:error service:billing").unwrap();
        h.remember(AB, SEARCH, "icon arrow up").unwrap();
        h.remember(LOGS, SEARCH, "service:Billing region:eu").unwrap();
        let found = |query: &str| h.search(query).iter().map(|input| input.text.clone()).collect::<Vec<_>>();
        assert_eq!(found("billing"), ["service:Billing region:eu", "level:error service:billing"]);
        assert_eq!(found("ERROR billing"), ["level:error service:billing"], "every word, any case");
        assert_eq!(found("  ").len(), 3, "a blank query lists the newest");
        assert!(found("nothing").is_empty());
    }

    #[test]
    fn completes_from_recent_inputs_and_searches_all() {
        let mut h = TempHistory::new("recent");
        h.file.remembered = (0..MAX_REMEMBERED_INPUTS).map(|i| input(format!("input {i:05}"))).collect();
        assert_eq!(h.completion_for("input 0000", 0), None, "too old to complete");
        assert_eq!(h.completion_for("input 099", 0).map(|c| c.remainder), Some("99"));
        assert_eq!(h.search("").len(), RECENT_INPUTS);
        assert_eq!(h.search("").first().map(|input| input.text.as_str()), Some("input 09999"));
        assert_eq!(h.search("00001").first().map(|input| input.text.as_str()), Some("input 00001"), "a query looks at all");
        assert_eq!(h.search("input").len(), MAX_SEARCH_RESULTS);
    }

    #[test]
    fn a_full_input_replaces_the_same_tools_partial_inputs() {
        let mut h = TempHistory::new("partials");
        h.remember(AB, SEARCH, "con").unwrap();
        for text in ["con", "cargo", "config", "config-manager"] {
            h.remember(LOGS, SEARCH, text).unwrap();
        }
        assert_eq!(h.texts(), ["con", "cargo", "config-manager"], "another plugin's `con` stays");
        h.remember(LOGS, SEARCH, "con").unwrap(); // a partial input used later is kept, as the newest
        assert_eq!(h.texts(), ["con", "cargo", "config-manager", "con"]);
        h.remember(LOGS, "tail", "config-manager-v2").unwrap();
        assert_eq!(h.texts(), ["con", "cargo", "config-manager", "con", "config-manager-v2"], "another tool's stay");
    }

    #[test]
    fn remembers_within_limits_and_persists() {
        let mut h = TempHistory::new("remember");
        for text in ["a", "b", "a", "  ", "b"] {
            h.remember(LOGS, SEARCH, text).unwrap();
        }
        assert_eq!(h.texts(), ["a", "b"]);
        h.remember(LOGS, SEARCH, &"é".repeat(MAX_INPUT_CHARS + 1)).unwrap();
        assert_eq!(h.texts(), ["a", "b"], "too-long inputs aren't remembered");
        h.remember(LOGS, SEARCH, &"é".repeat(MAX_INPUT_CHARS)).unwrap();
        assert_eq!(h.file.remembered.len(), 3, "the limit counts characters, not bytes");
        assert_eq!(h.reopened().file.remembered.len(), 3, "saved");
    }

    #[test]
    fn restores_the_last_input() {
        let mut h = TempHistory::new("restore");
        assert_eq!(h.input_to_restore(), None);
        h.set_input_to_restore("jwt eyJ…").unwrap();
        assert_eq!(h.reopened().input_to_restore(), Some("jwt eyJ…"));
        h.set_input_to_restore(&"x".repeat(MAX_INPUT_CHARS + 1)).unwrap();
        assert_eq!(h.input_to_restore(), None, "too long: not kept");
    }

    #[test]
    fn keeps_the_newest_within_count_and_size() {
        let mut h = TempHistory::new("limits");
        h.file.remembered = (0..MAX_REMEMBERED_INPUTS + 5).map(|i| input(i.to_string())).collect();
        h.remember(AB, SEARCH, "last").unwrap();
        assert_eq!(h.file.remembered.len(), MAX_REMEMBERED_INPUTS);
        assert_eq!(h.file.remembered[0].text, "6");

        let long = |i: usize| format!("{i:05}{}", "x".repeat(MAX_INPUT_CHARS - 10));
        h.file.remembered = (0..MAX_TOTAL_TEXT_BYTES / MAX_INPUT_CHARS + 20).map(|i| input(long(i))).collect();
        h.remember(AB, SEARCH, "newest").unwrap();
        let total: usize = h.file.remembered.iter().map(|input| input.text.len()).sum();
        assert!(total <= MAX_TOTAL_TEXT_BYTES, "{total}");
        assert_eq!(h.file.remembered.last().map(|input| input.text.as_str()), Some("newest"));
    }

    #[test]
    fn completing_from_a_full_history_is_fast() {
        let mut h = TempHistory::new("speed");
        h.file.remembered =
            (0..MAX_REMEMBERED_INPUTS).map(|i| input(format!("remembered input {i} of a typical length"))).collect();
        let started = std::time::Instant::now();
        for _ in 0..100 {
            assert_eq!(h.completion_for("no match anywhere", 0), None);
        }
        let per_keystroke = started.elapsed() / 100;
        assert!(per_keystroke < std::time::Duration::from_millis(5), "{per_keystroke:?}");
    }

    #[test]
    fn reads_the_previous_attempts_file() {
        let h = TempHistory::new("previous");
        std::fs::write(
            &h.path,
            r#"{"input_to_restore": "x", "remembered": [{"plugin_id": "acme.logs", "text": "old"}]}"#,
        )
        .unwrap();
        let reopened = h.reopened();
        assert_eq!(reopened.input_to_restore(), Some("x"));
        assert_eq!(reopened.file.remembered, [RememberedInput { plugin_id: LOGS.into(), operation_id: String::new(), text: "old".into() }]);
    }
}
