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

//! Phase 3: Runtime message key production.
//!
//! Given a parsed libyang data tree and a Phase 2 [`DerivationResult`],
//! walk the data tree, match instances to branch templates, extract key
//! leaf values, fill `%s` placeholders, and compose the final message
//! broker key.
//!
//! # Message key format
//!
//! The key is a line-delimited UTF-8 string:
//!
//! ```text
//! router-nyc-01
//! 1042
//! /ietf-interfaces:interfaces/interface[name='eth0'] | /ietf-interfaces:interfaces/interface[name='eth1']
//! ```
//!
//! | Line | Description                                         |
//! |------|-----------------------------------------------------|
//! | 1    | Managed node identifier (hostname, FQDN)            |
//! | 2    | YANG Push subscription ID                           |
//! | 3    | Concrete XPaths joined by ` \| `, sorted and deduped |
//!
//! This format guarantees byte-identical keys for identical inputs,
//! which is required for Message Broker topic compaction.

use std::collections::BTreeSet;

use yang4::data::{DataNodeRef, DataTree};
use yang4::schema::{SchemaNodeKind, SchemaPathFormat};

use crate::types::*;
use crate::xpath::strip_predicates;

// ------------------------------------------------------------------
//  Matching & extraction
// ------------------------------------------------------------------

/// Check whether the key template (with predicates stripped) matches
/// the data node's schema path.
fn template_matches_schema(template: &str, snode: &yang4::schema::SchemaNode) -> bool {
    let schema_path = snode.path(SchemaPathFormat::DATA);
    strip_predicates(template) == schema_path
}

/// Fill `%s` placeholders in a key template with actual values from
/// the data tree.
///
/// Each extraction spec contains an XPath query that is evaluated as
/// an optimized O(d) ancestor tree walk:
/// - `key_leaf_name == "."` → use the data node's own canonical value
///   (for leaf-list).
/// - Otherwise → walk up from `dnode` to find the ancestor list
///   matching `list_name`, then read the key leaf child's value.
fn fill_template(dnode: &DataNodeRef, branch: &BranchTemplate) -> Option<String> {
    let mut result = branch.key_template.clone();

    for ext in &branch.extractions {
        let value = if ext.key_leaf_name == "." {
            // Leaf-list: the data node's own value
            dnode.value_canonical()
        } else {
            // Optimized ancestor tree walk (equivalent to evaluating
            // the extraction XPath "ancestor-or-self::MOD:LIST/KEY")
            let list_node = dnode.inclusive_ancestors().find(|a| {
                a.schema().kind() == SchemaNodeKind::List && a.schema().name() == ext.list_name
            })?;

            list_node
                .children()
                .find(|c| c.schema().name() == ext.key_leaf_name)
                .and_then(|c| c.value_canonical())
        };

        let value = value?;
        result = result.replacen("%s", &value, 1);
    }

    Some(result)
}

// ------------------------------------------------------------------
//  Data tree walk
// ------------------------------------------------------------------

/// Recursively collect concrete keys from the data tree.
fn collect_keys(dnode: &DataNodeRef, derivation: &DerivationResult, keys: &mut Vec<String>) {
    for sibling in dnode.inclusive_siblings() {
        let snode = sibling.schema();

        // Check each branch template for a match
        for branch in &derivation.branches {
            if template_matches_schema(&branch.key_template, &snode)
                && let Some(ck) = fill_template(&sibling, branch)
                && !keys.contains(&ck)
            {
                keys.push(ck);
            }
        }

        // Recurse into children
        if let Some(child) = sibling.children().next() {
            collect_keys(&child, derivation, keys);
        }
    }
}

// ------------------------------------------------------------------
//  Public API
// ------------------------------------------------------------------

/// Produce the message broker key from a parsed notification data tree.
///
/// # Arguments
///
/// * `derivation` - Phase 2 output.
/// * `dtree` - Parsed libyang data tree (e.g. from `DataTree::parse_string`).
/// * `node_name` - Managed node identifier (hostname, FQDN, etc.).
/// * `subscription_id` - YANG Push subscription ID (e.g. `"1042"`).
///
/// # Key composition
///
/// 1. Walk the data tree and collect concrete XPaths for each matching
///    instance.
/// 2. Deduplicate and sort lexicographically.
/// 3. Build a [`MessageKey`] struct with `node_name`, `subscription_id`,
///    and the `xpaths` array.
/// 4. Serialize to line-delimited format for the `message_key` string.
///
/// # Errors
///
/// Returns `Err` if no matching instances are found (unless the
/// subscription targets a container with no list ancestors, in which
/// case the template itself is the concrete XPath).
pub fn produce_message_key(
    derivation: &DerivationResult,
    dtree: &DataTree,
    node_name: &str,
    subscription_id: &str,
) -> Result<MessageKeyResult, String> {
    let mut concrete_keys: Vec<String> = Vec::new();

    if let Some(dref) = dtree.reference() {
        collect_keys(&dref, derivation, &mut concrete_keys);
    }

    // For container-only subscriptions (no instances to find):
    // the template itself IS the concrete XPath.
    if concrete_keys.is_empty()
        && derivation.branches.len() == 1
        && derivation.branches[0].extractions.is_empty()
    {
        concrete_keys.push(derivation.branches[0].key_template.clone());
    }

    if concrete_keys.is_empty() {
        return Err("no matching instances in notification".into());
    }

    // Deduplicate and sort for deterministic ordering
    let unique: BTreeSet<String> = concrete_keys.into_iter().collect();
    let xpaths: Vec<String> = unique.into_iter().collect();

    let key = MessageKey {
        node_name: node_name.to_string(),
        subscription_id: subscription_id.to_string(),
        xpaths,
    };

    let message_key = key.to_line_delimited();

    Ok(MessageKeyResult { message_key, key })
}
