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
//!   produces a [`KafkaKeyResult`].

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
/// Each extraction maps a `%s` position (left-to-right) to the key
/// leaf that provides its runtime value.  The `list_module` and
/// `list_name` fields identify which ancestor list node to search
/// when walking the data tree in Phase 3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionSpec {
    /// Key leaf local name (e.g. `"name"`, `"id"`).
    /// For leaf-list targets this is `"."`.
    pub key_leaf_name: String,
    /// YANG module name of the owning list (e.g. `"ietf-interfaces"`).
    pub list_module: String,
    /// Local name of the owning list (e.g. `"interface"`).
    pub list_name: String,
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

/// Structured Kafka message key.
///
/// Serialized as compact JSON for use as the Kafka message key.
/// Field ordering is deterministic (alphabetical via serde's default
/// behavior), which guarantees byte-identical keys for identical
/// inputs — a requirement for Kafka compact-topic log compaction.
///
/// # JSON format
///
/// ```json
/// {
///   "node_name": "router-nyc-01",
///   "subscription_id": "1042",
///   "xpaths": [
///     "/ietf-interfaces:interfaces/interface[name='eth0']"
///   ]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KafkaKey {
    /// Administratively-assigned name identifying the managed node
    /// (e.g. hostname, FQDN).
    pub node_name: String,
    /// YANG Push subscription ID (e.g. `"1042"`).
    pub subscription_id: String,
    /// Sorted, deduplicated concrete XPaths extracted from the
    /// notification data tree.
    pub xpaths: Vec<String>,
}

/// Output of Phase 3 (runtime Kafka key production).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KafkaKeyResult {
    /// Compact JSON string — the actual Kafka message key.
    ///
    /// This is the serialized form of [`key`](Self::key) with no
    /// extra whitespace, suitable for direct use as a Kafka
    /// `ProducerRecord` key.
    pub kafka_key: String,
    /// Structured representation for programmatic access.
    pub key: KafkaKey,
}
