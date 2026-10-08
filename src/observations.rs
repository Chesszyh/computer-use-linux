use crate::atspi_tree::AccessibilityNode;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StateMode {
    #[default]
    Full,
    Diff,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TreeChanges {
    pub mode: String,
    pub added: Vec<u32>,
    pub changed: Vec<u32>,
    pub removed: Vec<u32>,
    pub total_nodes: usize,
}

#[derive(Default)]
pub struct Observations {
    next_index: u32,
    identities: BTreeMap<String, u32>,
    scopes: BTreeMap<String, Vec<AccessibilityNode>>,
    last_action: Option<Instant>,
}

impl Observations {
    pub fn reset(&mut self) {
        self.identities.clear();
        self.scopes.clear();
        self.last_action = None;
    }
    pub fn action_completed(&mut self) {
        self.last_action = Some(Instant::now());
    }

    pub fn remaining_settle_time(&self) -> Duration {
        self.last_action
            .map(|at| Duration::from_millis(150).saturating_sub(at.elapsed()))
            .unwrap_or_default()
    }

    pub fn node(&self, index: u32) -> Option<AccessibilityNode> {
        self.scopes
            .values()
            .flat_map(|nodes| nodes.iter())
            .find(|node| node.index == index)
            .cloned()
    }

    pub fn update(
        &mut self,
        scope: String,
        mut nodes: Vec<AccessibilityNode>,
        mode: StateMode,
        truncated: bool,
    ) -> (Vec<AccessibilityNode>, Vec<AccessibilityNode>, TreeChanges) {
        let mapping: BTreeMap<_, _> = nodes
            .iter()
            .map(|node| {
                let stable = *self
                    .identities
                    .entry(node.object_ref.clone())
                    .or_insert_with(|| {
                        let index = self.next_index;
                        self.next_index += 1;
                        index
                    });
                (node.index, stable)
            })
            .collect();
        for node in &mut nodes {
            node.index = mapping[&node.index];
            node.parent_index = node
                .parent_index
                .and_then(|parent| mapping.get(&parent).copied());
        }
        let previous = self.scopes.get(&scope);
        // An incomplete traversal cannot prove that missing nodes were removed.
        let use_diff = matches!(mode, StateMode::Diff) && previous.is_some() && !truncated;
        let mut changes = TreeChanges {
            mode: if use_diff { "diff" } else { "full" }.into(),
            added: Vec::new(),
            changed: Vec::new(),
            removed: Vec::new(),
            total_nodes: nodes.len(),
        };
        if use_diff {
            let before: BTreeMap<_, _> = previous
                .unwrap()
                .iter()
                .map(|node| (node.index, node))
                .collect();
            for node in &nodes {
                match before.get(&node.index) {
                    None => changes.added.push(node.index),
                    Some(old) if **old != *node => changes.changed.push(node.index),
                    _ => {}
                }
            }
            changes.removed = before
                .keys()
                .filter(|index| !nodes.iter().any(|node| node.index == **index))
                .copied()
                .collect();
        }
        let output = if use_diff {
            nodes
                .iter()
                .filter(|node| {
                    changes.added.contains(&node.index) || changes.changed.contains(&node.index)
                })
                .cloned()
                .collect()
        } else {
            nodes.clone()
        };
        if truncated {
            self.scopes.remove(&scope);
        } else {
            self.scopes.insert(scope, nodes.clone());
        }
        (nodes, output, changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(index: u32, id: &str, name: &str) -> AccessibilityNode {
        AccessibilityNode {
            index,
            object_ref: id.into(),
            name: Some(name.into()),
            parent_index: None,
            depth: 0,
            role: "button".into(),
            description: None,
            child_count: 0,
            bounds: None,
            states: vec![],
            actions: vec![],
            value: None,
            text: None,
            supports_editable_text: false,
        }
    }

    #[test]
    fn diff_survives_reordering_and_another_app_observation() {
        let mut store = Observations::default();
        let (first, _, _) = store.update(
            "a".into(),
            vec![node(0, "a", "A"), node(1, "b", "B")],
            StateMode::Diff,
            false,
        );
        store.update(
            "other".into(),
            vec![node(0, "c", "C")],
            StateMode::Diff,
            false,
        );
        let (all, delta, changes) = store.update(
            "a".into(),
            vec![node(0, "b", "B!"), node(1, "a", "A")],
            StateMode::Diff,
            false,
        );
        assert_eq!(all[0].index, first[1].index);
        assert_eq!(delta.len(), 1);
        assert_eq!(changes.changed, vec![first[1].index]);
        assert!(changes.added.is_empty());
        assert!(changes.removed.is_empty());
        let (_, unchanged, _) = store.update("a".into(), all, StateMode::Diff, false);
        assert!(unchanged.is_empty());
        assert_eq!(store.node(first[0].index).unwrap().object_ref, "a");
    }

    #[test]
    fn incomplete_tree_resets_baseline_and_removal_does_not_retarget_an_index() {
        let mut store = Observations::default();
        let (original, _, _) =
            store.update("a".into(), vec![node(0, "a", "A")], StateMode::Full, false);
        let (_, _, changes) =
            store.update("a".into(), vec![node(0, "b", "B")], StateMode::Diff, false);
        assert_eq!(changes.removed, vec![original[0].index]);
        assert!(store.node(original[0].index).is_none());
        let (_, _, changes) = store.update("a".into(), vec![], StateMode::Diff, true);
        assert_eq!(changes.mode, "full");
        assert!(changes.removed.is_empty());
        let (_, _, changes) = store.update("a".into(), vec![], StateMode::Diff, false);
        assert_eq!(changes.mode, "full");
    }
}
