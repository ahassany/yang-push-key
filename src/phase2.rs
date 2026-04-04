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

//! Phase 2: Key template derivation from subscription XPath.
//!
//! Given a subscription XPath and a compiled YANG schema context,
//! resolve each branch to its target schema node, walk the ancestor
//! chain from root to target, and build a key template with `%s`
//! placeholders for dynamic list keys and literal values for
//! statically-pinned keys.
//!
//! # Prefix convention
//!
//! The key template uses **minimal-prefix** style: the YANG module
//! name appears only when it differs from the previous segment.
//!
//! # Predicate handling
//!
//! | Predicate in subscription | Template output |
//! |---------------------------|-----------------|
//! | None (bare path) | `[key='%s']` + extraction |
//! | `[key='value']` | `[key='value']` (literal) |
//! | `[mod:key='value']` | `[key='value']` (prefix stripped) |
//! | `[key="value"]` | `[key='value']` (normalized) |
//! | `[N]` (positional) | `[key='%s']` (treated as open) |

use yang4::context::Context;
use yang4::schema::{SchemaNode, SchemaNodeKind};

use crate::types::*;
use crate::xpath::{XPathStep, escape_xpath_value, parse_xpath_steps, split_union};

// ------------------------------------------------------------------
//  Schema resolution
// ------------------------------------------------------------------

/// Resolve a subscription XPath branch to its target compiled schema node.
///
/// Tries `find_xpath` first (handles both full-prefix and minimal-prefix
/// styles; ignores predicates for schema resolution), then falls back to
/// `find_path`.
fn resolve_schema_node<'a>(ctx: &'a Context, branch_xpath: &str) -> Option<SchemaNode<'a>> {
    if let Ok(mut set) = ctx.find_xpath(branch_xpath)
        && let Some(snode) = set.next()
    {
        return Some(snode);
    }
    ctx.find_path(branch_xpath).ok()
}

/// Map a schema node kind to our target-type enum.
fn classify_target(snode: &SchemaNode) -> TargetType {
    match snode.kind() {
        SchemaNodeKind::List => TargetType::List,
        SchemaNodeKind::Leaf => TargetType::Leaf,
        SchemaNodeKind::LeafList => TargetType::LeafList,
        _ => TargetType::Container,
    }
}

// ------------------------------------------------------------------
//  Template building
// ------------------------------------------------------------------

/// Find the xpath step whose local name matches `node_name`.
///
/// Steps are consumed in order via `cursor`; this ensures each schema
/// node maps to the correct positional step even when names repeat.
fn find_matching_step<'a>(
    node_name: &str,
    steps: &'a [XPathStep],
    cursor: &mut usize,
) -> Option<&'a XPathStep> {
    while *cursor < steps.len() {
        if steps[*cursor].local_name == node_name {
            let step = &steps[*cursor];
            *cursor += 1;
            return Some(step);
        }
        *cursor += 1;
    }
    None
}

/// Build the key template string and extraction specs for one branch.
///
/// Algorithm:
/// 1. Collect the ancestor chain from target → root (skipping choice/case).
/// 2. Reverse to root → target order.
/// 3. For each node, emit a path segment with a minimal prefix.
/// 4. At each LIST node, check the original xpath step for pinned keys and emit literal or `%s` predicates accordingly.
/// 5. At a LEAF_LIST target, emit `[.='%s']` or a literal.
fn build_template(
    target: &SchemaNode,
    branch_xpath: &str,
) -> Result<(String, Vec<ExtractionSpec>, TargetType), String> {
    // Collect ancestors (target → root), skipping schema-only nodes
    let mut nodes: Vec<SchemaNode> = Vec::new();
    for ancestor in target.inclusive_ancestors() {
        match ancestor.kind() {
            SchemaNodeKind::Choice | SchemaNodeKind::Case => continue,
            _ => nodes.push(ancestor),
        }
    }
    nodes.reverse(); // root → target

    let xsteps = parse_xpath_steps(branch_xpath);
    let mut template = String::new();
    let mut extractions = Vec::new();
    let mut prev_mod: Option<String> = None;
    let mut xcursor = 0usize;

    for (i, node) in nodes.iter().enumerate() {
        let mod_name = node.module().name().to_string();
        let name = node.name().to_string();

        // Emit path segment with minimal prefix
        if prev_mod.as_deref() != Some(&mod_name) {
            template.push_str(&format!("/{}:{}", mod_name, name));
            prev_mod = Some(mod_name.clone());
        } else {
            template.push_str(&format!("/{}", name));
        }

        // Match to original xpath step (for predicate extraction)
        let xstep = find_matching_step(&name, &xsteps, &mut xcursor);

        // LIST: emit key predicates
        if node.kind() == SchemaNodeKind::List {
            for key_node in node.list_keys() {
                let key_name = key_node.name().to_string();

                // Check if this key is statically pinned in the subscription
                let pinned = xstep.and_then(|s| {
                    if s.has_positional {
                        return None; // positional does NOT pin
                    }
                    s.kvs
                        .iter()
                        .find(|kv| kv.key == key_name)
                        .map(|kv| kv.value.clone())
                });

                if let Some(val) = pinned {
                    template.push_str(&format!("[{}={}]", key_name, escape_xpath_value(&val)));
                } else {
                    template.push_str(&format!("[{}='%s']", key_name));
                    extractions.push(ExtractionSpec::for_list_key(
                        &key_name, &mod_name, &name, &template,
                    ));
                }
            }
        }

        // LEAF_LIST at target: emit value predicate
        if i == nodes.len() - 1 && node.kind() == SchemaNodeKind::LeafList {
            let pinned_dot = xstep.and_then(|s| {
                s.kvs
                    .iter()
                    .find(|kv| kv.key == ".")
                    .map(|kv| kv.value.clone())
            });
            if let Some(val) = pinned_dot {
                template.push_str(&format!("[.={}]", escape_xpath_value(&val)));
            } else {
                template.push_str("[.='%s']");
                extractions.push(ExtractionSpec::for_leaf_list_value(&mod_name, &name));
            }
        }
    }

    Ok((template, extractions, classify_target(target)))
}

// ------------------------------------------------------------------
//  Public API
// ------------------------------------------------------------------

/// Derive key templates from a subscription XPath.
///
/// The XPath may be a single path or multiple paths joined by `|`.
/// Each branch is independently resolved and templated.
///
/// # Errors
///
/// Returns `Err` if any branch cannot be resolved against the schema.
pub fn derive_templates(ctx: &Context, xpath: &str) -> Result<DerivationResult, String> {
    let branch_xpaths = split_union(xpath);
    let mut branches = Vec::new();

    for (i, bx) in branch_xpaths.iter().enumerate() {
        let target = resolve_schema_node(ctx, bx)
            .ok_or_else(|| format!("cannot resolve schema for '{}'", bx))?;
        let (key_template, extractions, target_type) = build_template(&target, bx)?;
        branches.push(BranchTemplate {
            branch_index: i,
            branch_xpath: bx.clone(),
            key_template,
            extractions,
            target_type,
        });
    }

    Ok(DerivationResult {
        subscription_xpath: xpath.to_string(),
        branches,
    })
}
