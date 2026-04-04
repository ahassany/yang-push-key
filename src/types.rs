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

use serde::{Deserialize, Serialize};

/// Classification of the subscription target schema node.
///
/// Determines how the key template is built and how instances are
/// matched at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
/// Each extraction is expressed as an absolute XPath that identifies
/// the key leaf in the data tree:
///
/// ```text
/// /MODULE:CONTAINER/.../LIST/KEY-LEAF-NAME
/// ```
///
/// The path mirrors the template path from the root to the owning
/// list (preserving any pinned predicates on ancestor lists) and
/// appends the key leaf name without a predicate.
///
/// For leaf-list targets the extraction XPath is `"."`.
///
/// At runtime (Phase 3) the extraction can be optimized to an
/// equivalent `ancestor-or-self` tree walk with O(d) complexity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionSpec {
    /// Absolute XPath identifying the key leaf in the data tree.
    ///
    /// Examples:
    /// - `"/ietf-interfaces:interfaces/interface/name"`
    /// - `"/ietf-interfaces:interfaces/interface[name='eth0']/ietf-ip:ipv4/address/ip"`
    /// - `"."` (leaf-list own value)
    pub extraction_xpath: String,

    // -- Fields for the optimized Phase 3 ancestor tree walk --
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
    /// `template_prefix` is the key template built so far up to and
    /// including the owning list node (with any pinned predicates on
    /// ancestor lists). The extraction XPath is formed by stripping
    /// the list's own key predicates and appending `/key_leaf_name`.
    pub fn for_list_key(
        key_leaf_name: &str,
        list_module: &str,
        list_name: &str,
        template_prefix: &str,
    ) -> Self {
        // Strip predicates from the last segment (the list node) to
        // build a clean extraction path:
        //   "/a:x/y[name='%s']"  →  "/a:x/y/key_leaf"
        let base = strip_last_predicates(template_prefix);
        let extraction_xpath = format!("{}/{}", base, key_leaf_name);
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

    /// Parse an extraction XPath string and derive the optimized
    /// tree-walk fields.
    ///
    /// Accepts:
    /// - `"."` (leaf-list)
    /// - An absolute XPath like `"/a:containers/list/key_leaf"`
    ///
    /// The list module and name are derived from the second-to-last
    /// path segment, and the key leaf from the last segment.
    pub fn from_xpath(xpath: &str) -> Result<Self, String> {
        if xpath == "." {
            return Ok(Self {
                extraction_xpath: ".".to_string(),
                key_leaf_name: ".".to_string(),
                list_module: String::new(),
                list_name: String::new(),
            });
        }

        if !xpath.starts_with('/') {
            return Err(format!("extraction xpath must be absolute: '{}'", xpath));
        }

        // Split on '/' to get segments; last is KEY, second-to-last is LIST
        let (parent_path, key_leaf) = xpath
            .rsplit_once('/')
            .ok_or_else(|| format!("missing key leaf in extraction xpath: '{}'", xpath))?;

        // Find the last segment of parent_path (the list node name)
        let list_segment = parent_path
            .rsplit_once('/')
            .map(|(_, seg)| seg)
            .unwrap_or(parent_path);

        // Strip any predicates from the list segment: "list[k='v']" → "list"
        let list_local = list_segment
            .split_once('[')
            .map(|(name, _)| name)
            .unwrap_or(list_segment);

        // Extract module prefix if present: "mod:name" → ("mod", "name")
        let (module, list_name) = if let Some((m, n)) = list_local.split_once(':') {
            (m.to_string(), n.to_string())
        } else {
            // No module prefix on the list segment — try to find one
            // by walking backwards through earlier segments
            let mut found_module = String::new();
            for seg in parent_path.split('/').rev() {
                let clean = seg.split_once('[').map(|(n, _)| n).unwrap_or(seg);
                if let Some((m, _)) = clean.split_once(':') {
                    found_module = m.to_string();
                    break;
                }
            }
            (found_module, list_local.to_string())
        };

        Ok(Self {
            extraction_xpath: xpath.to_string(),
            key_leaf_name: key_leaf.to_string(),
            list_module: module,
            list_name,
        })
    }
}

/// Strip predicates (`[...]`) from the last path segment only.
///
/// For example: `/a:x/y[name='%s'][id='%s']` → `/a:x/y`
fn strip_last_predicates(path: &str) -> String {
    if let Some(last_slash) = path.rfind('/') {
        let prefix = &path[..last_slash];
        let last_seg = &path[last_slash + 1..];
        let clean = last_seg
            .split_once('[')
            .map(|(name, _)| name)
            .unwrap_or(last_seg);
        format!("{}/{}", prefix, clean)
    } else {
        path.to_string()
    }
}

/// Key template and extraction metadata for one branch of a
/// (possibly union) subscription XPath.
///
/// A subscription like `"/a:x/y | /b:p/q"` produces two branches.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
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
        format!(
            "{}\n{}\n{}",
            self.node_name, self.subscription_id, xpaths_line
        )
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
    fn extraction_for_list_key_builds_absolute_xpath() {
        let ext = ExtractionSpec::for_list_key(
            "name",
            "ietf-interfaces",
            "interface",
            "/ietf-interfaces:interfaces/interface[name='%s']",
        );
        assert_eq!(
            ext.extraction_xpath,
            "/ietf-interfaces:interfaces/interface/name"
        );
        assert_eq!(ext.key_leaf_name, "name");
        assert_eq!(ext.list_module, "ietf-interfaces");
        assert_eq!(ext.list_name, "interface");
    }

    #[test]
    fn extraction_for_list_key_with_pinned_ancestor() {
        let ext = ExtractionSpec::for_list_key(
            "ip",
            "ietf-ip",
            "address",
            "/ietf-interfaces:interfaces/interface[name='eth0']/ietf-ip:ipv4/address[ip='%s']",
        );
        assert_eq!(
            ext.extraction_xpath,
            "/ietf-interfaces:interfaces/interface[name='eth0']/ietf-ip:ipv4/address/ip"
        );
        assert_eq!(ext.key_leaf_name, "ip");
        assert_eq!(ext.list_module, "ietf-ip");
        assert_eq!(ext.list_name, "address");
    }

    #[test]
    fn extraction_for_leaf_list_value() {
        let ext = ExtractionSpec::for_leaf_list_value("ietf-system", "search");
        assert_eq!(ext.extraction_xpath, ".");
        assert_eq!(ext.key_leaf_name, ".");
    }

    #[test]
    fn from_xpath_parses_absolute_path() {
        let expected = ExtractionSpec {
            extraction_xpath: "/ietf-interfaces:interfaces/interface/name".to_string(),
            key_leaf_name: "name".to_string(),
            list_module: "ietf-interfaces".to_string(),
            list_name: "interface".to_string(),
        };

        let sepc =
            ExtractionSpec::from_xpath("/ietf-interfaces:interfaces/interface/name").unwrap();
        assert_eq!(sepc, expected);
    }

    #[test]
    fn from_xpath_parses_nested_with_pinned_predicate() {
        let expected = ExtractionSpec {
            extraction_xpath:
                "/ietf-interfaces:interfaces/interface[name='eth0']/ietf-ip:ipv4/address/ip"
                    .to_string(),
            key_leaf_name: "ip".to_string(),
            list_module: "ietf-ip".to_string(),
            list_name: "address".to_string(),
        };

        let spec = ExtractionSpec::from_xpath(
            "/ietf-interfaces:interfaces/interface[name='eth0']/ietf-ip:ipv4/address/ip",
        )
        .unwrap();

        assert_eq!(spec, expected);
    }

    #[test]
    fn from_xpath_parses_dot() {
        let expected = ExtractionSpec {
            extraction_xpath: ".".to_string(),
            key_leaf_name: ".".to_string(),
            list_module: "".to_string(),
            list_name: "".to_string(),
        };
        let spec = ExtractionSpec::from_xpath(".").unwrap();
        assert_eq!(spec, expected);
    }

    #[test]
    fn from_xpath_rejects_invalid() {
        assert!(ExtractionSpec::from_xpath("invalid").is_err());
    }

    #[test]
    fn from_xpath_roundtrip_matches_for_list_key() {
        let original = ExtractionSpec::for_list_key(
            "id",
            "example-network",
            "interface",
            "/example-network:network-instances/network-instance[name='%s']/interface[id='%s']",
        );
        let parsed = ExtractionSpec::from_xpath(&original.extraction_xpath).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn strip_last_predicates_basic() {
        assert_eq!(strip_last_predicates("/a:x/y[name='%s']"), "/a:x/y");
    }

    #[test]
    fn strip_last_predicates_composite() {
        assert_eq!(
            strip_last_predicates("/a:x/route[dst='%s'][nh='%s']"),
            "/a:x/route"
        );
    }

    #[test]
    fn strip_last_predicates_no_predicates() {
        assert_eq!(strip_last_predicates("/a:x/y"), "/a:x/y");
    }

    #[test]
    fn message_key_single_xpath() {
        let key = MessageKey {
            node_name: "router-nyc-01".into(),
            subscription_id: "1042".into(),
            xpaths: vec!["/ietf-interfaces:interfaces/interface[name='eth0']".into()],
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
