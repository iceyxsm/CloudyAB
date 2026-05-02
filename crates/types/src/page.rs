//! Page structure and accessibility tree types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A snapshot of a page's accessibility tree with element refs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSnapshot {
    /// The page URL
    pub url: String,
    /// Page title
    pub title: String,
    /// Text representation of the accessibility tree
    pub tree: String,
    /// Map of ref IDs to element metadata
    pub refs: HashMap<String, ElementRef>,
}

/// A reference to an interactive element on the page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementRef {
    /// CSS selector or locator strategy to find this element
    pub selector: String,
    /// ARIA role (button, link, textbox, etc.)
    pub role: String,
    /// Accessible name (visible text or aria-label)
    pub name: String,
    /// Disambiguation index for duplicate role+name combos
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nth: Option<u32>,
    /// Additional attributes
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, String>,
}

/// ARIA roles that are always interactive (always get refs).
pub const INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "link",
    "textbox",
    "checkbox",
    "radio",
    "combobox",
    "listbox",
    "menuitem",
    "searchbox",
    "slider",
    "spinbutton",
    "switch",
    "tab",
    "treeitem",
];

/// Content roles that get refs only if they have a name.
pub const CONTENT_ROLES: &[&str] = &[
    "heading",
    "cell",
    "gridcell",
    "columnheader",
    "rowheader",
    "listitem",
    "article",
    "region",
    "main",
    "navigation",
];

/// Options for generating a page snapshot.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnapshotOptions {
    /// Only include interactive elements
    #[serde(default)]
    pub interactive_only: bool,
    /// Remove empty structural elements
    #[serde(default)]
    pub compact: bool,
    /// Maximum tree depth (0 = unlimited)
    #[serde(default)]
    pub max_depth: u32,
    /// CSS selector to scope the snapshot
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
}
