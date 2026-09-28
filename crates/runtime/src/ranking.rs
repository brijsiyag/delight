//! Which tools fit an input: every plugin's detections, ranked into one list.

use std::collections::HashMap;

use delight_manifest::Manifest;
use delight_protocol::Detection;

/// Detections at or above this confidence are recommended: the best of them opens
/// by itself, and the list's first group is headed "Recommended".
pub const RECOMMENDED: f32 = 0.5;

/// One tool that fits the input: operation `operation` of plugin `plugin`, as indexes
/// into the plugins given to [`rank`] and that plugin's manifest's operations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub plugin: usize,
    pub operation: usize,
    /// Above 0, at most 1.
    pub confidence: f32,
}

impl Candidate {
    pub fn is_recommended(&self) -> bool {
        self.confidence >= RECOMMENDED
    }
}

/// Rank what each plugin detected into the tool list, best first.
///
/// `plugins` are in the app's order (built-ins by id, then installed ones by path),
/// each with its manifest and its detections. A detection counts only with a
/// confidence above 0 (at most 1: more counts as 1) and an operation id its manifest
/// lists; an unknown id is logged and dropped. For each operation the best detection
/// is kept. Ties go to the plugin, then the operation, listed first.
pub fn rank<'a>(
    plugins: impl IntoIterator<Item = (&'a Manifest, &'a [Detection])>,
) -> Vec<Candidate> {
    let mut best: HashMap<(usize, usize), f32> = HashMap::new();
    for (plugin, (manifest, detections)) in plugins.into_iter().enumerate() {
        for detection in detections {
            if detection.confidence.is_nan() || detection.confidence <= 0.0 {
                continue;
            }
            let Some(operation) = manifest
                .operations
                .iter()
                .position(|operation| operation.id == detection.operation)
            else {
                log::warn!(
                    "plugin {} detected {:?}, which its manifest doesn't list; ignored",
                    manifest.plugin.id,
                    detection.operation,
                );
                continue;
            };
            let confidence = detection.confidence.min(1.0);
            let entry = best.entry((plugin, operation)).or_insert(confidence);
            *entry = entry.max(confidence);
        }
    }
    let mut candidates: Vec<Candidate> = best
        .into_iter()
        .map(|((plugin, operation), confidence)| Candidate {
            plugin,
            operation,
            confidence,
        })
        .collect();
    candidates.sort_by(|a, b| {
        b.confidence
            .total_cmp(&a.confidence)
            .then(a.plugin.cmp(&b.plugin))
            .then(a.operation.cmp(&b.operation))
    });
    candidates
}

#[cfg(test)]
mod tests {
    use delight_manifest::{Operation, PluginProperties};

    use super::*;

    fn manifest(id: &str, operations: &[&str]) -> Manifest {
        Manifest {
            plugin: PluginProperties {
                id: id.into(),
                name: id.into(),
                version: "1".into(),
                description: String::new(),
                author: String::new(),
                icon: "<svg/>".into(),
                tags: Vec::new(),
                permissions: Vec::new(),
            },
            operations: operations
                .iter()
                .map(|id| Operation {
                    id: (*id).into(),
                    title: (*id).into(),
                    description: String::new(),
                    icon: None,
                    tags: Vec::new(),
                })
                .collect(),
        }
    }

    fn detection(operation: &str, confidence: f32) -> Detection {
        Detection {
            operation: operation.into(),
            confidence,
        }
    }

    /// (plugin, operation, confidence) triples, for short assertions.
    fn ranked(plugins: &[(&Manifest, Vec<Detection>)]) -> Vec<(usize, usize, f32)> {
        rank(plugins.iter().map(|(manifest, detections)| (*manifest, detections.as_slice())))
            .into_iter()
            .map(|candidate| (candidate.plugin, candidate.operation, candidate.confidence))
            .collect()
    }

    #[test]
    fn best_first_and_ties_in_plugin_then_operation_order() {
        let json = manifest("json", &["format", "minify"]);
        let yaml = manifest("yaml", &["convert"]);
        let result = ranked(&[
            (&json, vec![detection("minify", 0.6), detection("format", 0.6)]),
            (&yaml, vec![detection("convert", 0.9)]),
        ]);
        assert_eq!(result, [(1, 0, 0.9), (0, 0, 0.6), (0, 1, 0.6)]);
    }

    #[test]
    fn only_positive_confidences_count_and_more_than_one_is_one() {
        let json = manifest("json", &["format", "minify", "escape"]);
        let result = ranked(&[(
            &json,
            vec![
                detection("format", 0.0),
                detection("minify", -1.0),
                detection("escape", f32::NAN),
                detection("format", 3.0),
            ],
        )]);
        assert_eq!(result, [(0, 0, 1.0)]);
    }

    #[test]
    fn unknown_operations_are_dropped() {
        let json = manifest("json", &["format"]);
        let result = ranked(&[(&json, vec![detection("nope", 0.9), detection("format", 0.4)])]);
        assert_eq!(result, [(0, 0, 0.4)]);
    }

    #[test]
    fn each_operation_keeps_its_best_detection() {
        let json = manifest("json", &["format"]);
        let result = ranked(&[(
            &json,
            vec![detection("format", 0.3), detection("format", 0.8), detection("format", 0.5)],
        )]);
        assert_eq!(result, [(0, 0, 0.8)]);
    }

    #[test]
    fn recommended_from_one_half() {
        let at = Candidate {
            plugin: 0,
            operation: 0,
            confidence: RECOMMENDED,
        };
        let below = Candidate {
            confidence: 0.49,
            ..at
        };
        assert!(at.is_recommended());
        assert!(!below.is_recommended());
    }

    #[test]
    fn nothing_detected_is_an_empty_list() {
        let json = manifest("json", &["format"]);
        assert!(ranked(&[(&json, Vec::new())]).is_empty());
    }
}
