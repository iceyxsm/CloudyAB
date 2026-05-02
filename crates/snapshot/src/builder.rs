//! Snapshot builder — constructs accessibility tree with refs.

use cloudyab_types::page::{
    ElementRef, PageSnapshot, SnapshotOptions, CONTENT_ROLES, INTERACTIVE_ROLES,
};
use std::collections::HashMap;

/// Builds a page snapshot with @eN refs from DOM data.
pub struct SnapshotBuilder {
    ref_counter: u32,
    refs: HashMap<String, ElementRef>,
    tree_lines: Vec<String>,
}

impl SnapshotBuilder {
    /// Create a new snapshot builder.
    pub fn new() -> Self {
        Self {
            ref_counter: 0,
            refs: HashMap::new(),
            tree_lines: Vec::new(),
        }
    }

    /// Add an element to the snapshot.
    ///
    /// Returns the assigned ref ID if the element is interactive or named content.
    pub fn add_element(
        &mut self,
        role: &str,
        name: &str,
        selector: &str,
        depth: u32,
        attributes: HashMap<String, String>,
        options: &SnapshotOptions,
    ) -> Option<String> {
        // Check depth limit
        if options.max_depth > 0 && depth > options.max_depth {
            return None;
        }

        let is_interactive = INTERACTIVE_ROLES.contains(&role);
        let is_named_content = CONTENT_ROLES.contains(&role) && !name.is_empty();

        // In interactive-only mode, skip non-interactive elements
        if options.interactive_only && !is_interactive {
            return None;
        }

        // Assign a ref if interactive or named content
        let ref_id = if is_interactive || is_named_content {
            self.ref_counter += 1;
            let id = format!("e{}", self.ref_counter);

            self.refs.insert(
                id.clone(),
                ElementRef {
                    selector: selector.to_string(),
                    role: role.to_string(),
                    name: name.to_string(),
                    nth: None,
                    attributes: attributes.clone(),
                },
            );

            Some(id)
        } else {
            None
        };

        // Build tree line
        let indent = "  ".repeat(depth as usize);
        let ref_str = ref_id
            .as_ref()
            .map(|id| format!(" [ref={id}]"))
            .unwrap_or_default();

        let name_str = if name.is_empty() {
            String::new()
        } else {
            format!(" \"{name}\"")
        };

        let attr_str = if attributes.is_empty() {
            String::new()
        } else {
            let attrs: Vec<_> = attributes
                .iter()
                .map(|(k, v)| format!("[{k}={v}]"))
                .collect();
            format!(" {}", attrs.join(" "))
        };

        let line = format!("{indent}- {role}{name_str}{ref_str}{attr_str}");

        if !options.compact || !name.is_empty() || ref_id.is_some() {
            self.tree_lines.push(line);
        }

        ref_id
    }

    /// Build the final snapshot.
    pub fn build(self, url: &str, title: &str) -> PageSnapshot {
        PageSnapshot {
            url: url.to_string(),
            title: title.to_string(),
            tree: self.tree_lines.join("\n"),
            refs: self.refs,
        }
    }

    /// Get the current ref count.
    pub fn ref_count(&self) -> u32 {
        self.ref_counter
    }
}

impl Default for SnapshotBuilder {
    fn default() -> Self {
        Self::new()
    }
}
