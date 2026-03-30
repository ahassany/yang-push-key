// Copyright (C) 2026-present Ahmed Elhassany.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
// implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Shared data structures for the YANG Push key derivation algorithm.
//!
//! These types are the interface between Phases 1, 2, and 3:
//!
//! - Phase 1 produces an XPath `String`.
//! - Phase 2 consumes that XPath and produces a [`DerivationResult`].
//! - Phase 3 consumes the [`DerivationResult`] plus a data tree and
//!   produces a [`MessageKeyResult`].

use serde::Serialize;

/// Classification of the subscription target schema node.
///
/// Determines how the key template is built and how instances are
/// matched at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetType {
    /// A YANG `container`.  No list predicates at this level.
    Container,
    /// A YANG `list`.  Key predicates are appended for this node.
    List,
    /// A YANG `leaf` (inside a list or container).
    Leaf,
    /// A YANG `leaf-list`.  Value predicate `[.='%s']` is appended.
    LeafList,
}

/// Describes how to fill one `%s` placeholder in a key template.
///
/// Each extraction is expressed as an XPath query using the
/// `ancestor-or-self` axis:
///
/// ```text
/// ancestor-or-self::MODULE:LIST-NAME/KEY-LEAF-NAME
/// ```
///
/// For leaf-list targets the extraction XPath is `"."`.
///
/// At runtime the extraction is evaluated as an optimized upward tree
/// walk with O(d) complexity (where d is the depth of the data tree),
/// rather than invoking a full XPath evaluator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionSpec {
    /// XPath query for extracting the key value at runtime.
    ///
    /// Examples:
    /// - `"ancestor-or-self::ietf-interfaces:interface/name"`
    /// - `"."` (leaf-list own value)
    pub extraction_xpath: String,

    // -- Optimized fields derived from extraction_xpath --

    /// Key leaf local name (e.g. `"name"`, `"id"`).
    /// For leaf-list targets this is `"."`.
    pub key_leaf_name: String,
    /// YANG module name of the owning list (e.g. `"ietf-interfaces"`).
    /// Empty for leaf-list targets (`"."`).
    pub list_module: String,
    /// Local name of the owning list (e.g. `"interface"`).
    /// Empty for leaf-list targets (`"."`).
    pub list_name: String,
}

impl ExtractionSpec {
    /// Create an extraction for a list key leaf.
    ///
    /// Builds the canonical XPath `ancestor-or-self::MODULE:LIST/KEY`
    /// and stores the optimized fields for O(d) tree-walk extraction.
    pub fn for_list_key(key_leaf_name: &str, list_module: &str, list_name: &str) -> Self {
        let extraction_xpath = format!(
            "ancestor-or-self::{}:{}/{}",
            list_module, list_name, key_leaf_name
        );
        Self {
            extraction_xpath,
            key_leaf_name: key_leaf_name.to_string(),
            list_module: list_module.to_string(),
            list_name: list_name.to_string(),
        }
    }

    /// Create an extraction for a leaf-list's own value.
    pub fn for_leaf_list_value(list_module: &str, list_name: &str) -> Self {
        Self {
            extraction_xpath: ".".to_string(),
            key_leaf_name: ".".to_string(),
            list_module: list_module.to_string(),
            list_name: list_name.to_string(),
        }
    }

    /// Parse an extraction XPath string and derive the optimized fields.
    ///
    /// Accepts either `"."` (leaf-list) or
    /// `"ancestor-or-self::MODULE:LIST/KEY"`.
    ///
    /// This allows Phase 3 to accept XPath-based extraction specs
    /// and compute the optimized tree-walk parameters on the fly.
    pub fn from_xpath(xpath: &str) -> Result<Self, String> {
        if xpath == "." {
            return Ok(Self {
                extraction_xpath: ".".to_string(),
                key_leaf_name: ".".to_string(),
                list_module: String::new(),
                list_name: String::new(),
            });
        }

        // Expected: "ancestor-or-self::MODULE:LIST/KEY"
        let rest = xpath
            .strip_prefix("ancestor-or-self::")
            .ok_or_else(|| format!("invalid extraction xpath: '{}'", xpath))?;

        // Split on '/' to get "MODULE:LIST" and "KEY"
        let (module_list, key_leaf) = rest
            .rsplit_once('/')
            .ok_or_else(|| format!("missing key leaf in extraction xpath: '{}'", xpath))?;

        // Split "MODULE:LIST" on ':' to get MODULE and LIST
        let (module, list) = module_list
            .split_once(':')
            .ok_or_else(|| format!("missing module prefix in extraction xpath: '{}'", xpath))?;

        Ok(Self {
            extraction_xpath: xpath.to_string(),
            key_leaf_name: key_leaf.to_string(),
            list_module: module.to_string(),
            list_name: list.to_string(),
        })
    }
}

/// Key template and extraction metadata for one branch of a
/// (possibly union) subscription XPath.
///
/// A subscription like `"/a:x/y | /b:p/q"` produces two branches.
#[derive(Debug, Clone)]
pub struct BranchTemplate {
    /// Zero-based position in the union.
    pub branch_index: usize,
    /// The original XPath for this branch (before template derivation).
    pub branch_xpath: String,
    /// The key template with `%s` placeholders for dynamic keys and
    /// literal values for statically-pinned keys.
    ///
    /// Example: `/ietf-interfaces:interfaces/interface[name='%s']`
    pub key_template: String,
    /// One entry per `%s` in `key_template`, in left-to-right order.
    pub extractions: Vec<ExtractionSpec>,
    /// What kind of schema node the subscription targets.
    pub target_type: TargetType,
}

/// Complete output of Phase 2 (key template derivation).
#[derive(Debug, Clone)]
pub struct DerivationResult {
    /// The full subscription XPath (original or normalized from Phase 1).
    pub subscription_xpath: String,
    /// One [`BranchTemplate`] per union branch.
    pub branches: Vec<BranchTemplate>,
}

/// Structured message key.
///
/// The message key uses a line-delimited format:
///
/// ```text
/// <node_name>\n<subscription_id>\n<xpath1> | <xpath2> | ...
/// ```
///
/// Field ordering is fixed and XPaths are sorted lexicographically,
/// which guarantees byte-identical keys for identical inputs — a
/// requirement for Message Broker topic compaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MessageKey {
    /// Administratively-assigned name identifying the managed node
    /// (e.g. hostname, FQDN).
    pub node_name: String,
    /// YANG Push subscription ID (e.g. `"1042"`).
    pub subscription_id: String,
    /// Sorted, deduplicated concrete XPaths extracted from the
    /// notification data tree.
    pub xpaths: Vec<String>,
}

impl MessageKey {
    /// Serialize to the line-delimited message key format.
    ///
    /// Format: `node_name \n subscription_id \n xpath1 | xpath2 | ...`
    pub fn to_line_delimited(&self) -> String {
        let xpaths_line = self.xpaths.join(" | ");
        format!("{}\n{}\n{}", self.node_name, self.subscription_id, xpaths_line)
    }
}

/// Output of Phase 3 (runtime message key production).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageKeyResult {
    /// Line-delimited string — the actual message broker key.
    ///
    /// Format: `node_name \n subscription_id \n xpath1 | xpath2 | ...`
    ///
    /// This is the serialized form of [`key`](Self::key) suitable for
    /// direct use as a Message Broker record key.
    pub message_key: String,
    /// Structured representation for programmatic access.
    pub key: MessageKey,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_for_list_key_builds_xpath() {
        let ext = ExtractionSpec::for_list_key("name", "ietf-interfaces", "interface");
        assert_eq!(
            ext.extraction_xpath,
            "ancestor-or-self::ietf-interfaces:interface/name"
        );
        assert_eq!(ext.key_leaf_name, "name");
        assert_eq!(ext.list_module, "ietf-interfaces");
        assert_eq!(ext.list_name, "interface");
    }

    #[test]
    fn extraction_for_leaf_list_value() {
        let ext = ExtractionSpec::for_leaf_list_value("ietf-system", "search");
        assert_eq!(ext.extraction_xpath, ".");
        assert_eq!(ext.key_leaf_name, ".");
    }

    #[test]
    fn from_xpath_parses_ancestor_query() {
        let ext =
            ExtractionSpec::from_xpath("ancestor-or-self::ietf-interfaces:interface/name")
                .unwrap();
        assert_eq!(
            ext.extraction_xpath,
            "ancestor-or-self::ietf-interfaces:interface/name"
        );
        assert_eq!(ext.key_leaf_name, "name");
        assert_eq!(ext.list_module, "ietf-interfaces");
        assert_eq!(ext.list_name, "interface");
    }

    #[test]
    fn from_xpath_parses_dot() {
        let ext = ExtractionSpec::from_xpath(".").unwrap();
        assert_eq!(ext.extraction_xpath, ".");
        assert_eq!(ext.key_leaf_name, ".");
    }

    #[test]
    fn from_xpath_rejects_invalid() {
        assert!(ExtractionSpec::from_xpath("invalid").is_err());
    }

    #[test]
    fn from_xpath_roundtrip_matches_for_list_key() {
        let original = ExtractionSpec::for_list_key("id", "example-network", "interface");
        let parsed = ExtractionSpec::from_xpath(&original.extraction_xpath).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn message_key_single_xpath() {
        let key = MessageKey {
            node_name: "router-nyc-01".into(),
            subscription_id: "1042".into(),
            xpaths: vec![
                "/ietf-interfaces:interfaces/interface[name='eth0']".into(),
            ],
        };
        assert_eq!(
            key.to_line_delimited(),
            "router-nyc-01\n1042\n/ietf-interfaces:interfaces/interface[name='eth0']"
        );
    }

    #[test]
    fn message_key_multiple_xpaths_joined_by_pipe() {
        let key = MessageKey {
            node_name: "router-nyc-01".into(),
            subscription_id: "1042".into(),
            xpaths: vec![
                "/ietf-interfaces:interfaces/interface[name='eth0']".into(),
                "/ietf-interfaces:interfaces/interface[name='eth1']".into(),
            ],
        };
        let expected = "router-nyc-01\n1042\n\
            /ietf-interfaces:interfaces/interface[name='eth0'] | \
            /ietf-interfaces:interfaces/interface[name='eth1']";
        assert_eq!(key.to_line_delimited(), expected);
    }
}
