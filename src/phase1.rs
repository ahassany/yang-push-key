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

//! Phase 1: Subtree filter normalization to XPath.
//!
//! Converts a YANG Push subtree filter (RFC 6241 §6) into one or more
//! equivalent XPath expressions joined by ` | `.
//!
//! # Element classification (RFC 6241 §6.2)
//!
//! | Kind | Has text? | Has children? | Treatment |
//! |------|-----------|---------------|-----------|
//! | Content match | yes | no | XPath equality predicate |
//! | Selection | no | no | Terminal branch leaf |
//! | Containment | — | yes | Path step, recurse |
//!
//! # Output format
//!
//! Every path segment carries the full YANG module-name prefix
//! (e.g. `/ietf-interfaces:interfaces/ietf-interfaces:interface`).
//! Phase 2 accepts this and normalizes to minimal-prefix style.

use std::collections::BTreeSet;

use quick_xml::Reader;
use quick_xml::events::Event;
use yang4::context::Context;

use crate::xpath::escape_xpath_value;

// ------------------------------------------------------------------
//  Lightweight XML tree (for recursive walking)
// ------------------------------------------------------------------

/// An XML element with optional namespace, text, and children.
#[derive(Debug, Clone)]
struct XmlElem {
    local_name: String,
    namespace: Option<String>,
    text: Option<String>,
    children: Vec<XmlElem>,
}

// ------------------------------------------------------------------
//  XML parsing (quick-xml 0.39)
// ------------------------------------------------------------------

/// Parse an XML string into a flat list of root elements.
fn parse_xml_to_tree(xml: &str) -> Result<Vec<XmlElem>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut stack: Vec<XmlElem> = Vec::new();
    let mut roots: Vec<XmlElem> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let (local, ns) = extract_name_ns(e);
                let inherited_ns = stack.last().and_then(|p| p.namespace.clone());
                stack.push(XmlElem {
                    local_name: local,
                    namespace: ns.or(inherited_ns),
                    text: None,
                    children: Vec::new(),
                });
            }
            Ok(Event::Empty(ref e)) => {
                let (local, ns) = extract_name_ns(e);
                let inherited_ns = stack.last().and_then(|p| p.namespace.clone());
                let elem = XmlElem {
                    local_name: local,
                    namespace: ns.or(inherited_ns),
                    text: None,
                    children: Vec::new(),
                };
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(elem);
                } else {
                    roots.push(elem);
                }
            }
            Ok(Event::Text(ref e)) => {
                // quick-xml 0.39: BytesText gives raw unescaped text
                if let Some(current) = stack.last_mut() {
                    let chunk = String::from_utf8_lossy(e).trim().to_string();
                    if !chunk.is_empty() {
                        match &mut current.text {
                            Some(existing) => existing.push_str(&chunk),
                            None => current.text = Some(chunk),
                        }
                    }
                }
            }
            Ok(Event::GeneralRef(ref e)) => {
                // quick-xml 0.39: entity references produce GeneralRef events
                if let Some(current) = stack.last_mut() {
                    let entity = String::from_utf8_lossy(e);
                    let resolved = match entity.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => "",
                    };
                    if !resolved.is_empty() {
                        match &mut current.text {
                            Some(existing) => existing.push_str(resolved),
                            None => current.text = Some(resolved.to_string()),
                        }
                    }
                }
            }
            Ok(Event::End(_)) => {
                if let Some(elem) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(elem);
                    } else {
                        roots.push(elem);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
    }
    Ok(roots)
}

/// Extract the local name and `xmlns` default-namespace from a start tag.
fn extract_name_ns(e: &quick_xml::events::BytesStart) -> (String, Option<String>) {
    let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
    let mut ns: Option<String> = None;
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        if key == "xmlns" {
            ns = Some(
                attr.unescape_value()
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
            );
        }
    }
    (local, ns)
}

// ------------------------------------------------------------------
//  Namespace → YANG module resolution
// ------------------------------------------------------------------

/// Resolve an XML namespace URI to its YANG module name.
fn resolve_ns(ctx: &Context, ns_uri: &str) -> Result<String, String> {
    ctx.get_module_implemented_ns(ns_uri)
        .map(|m| m.name().to_string())
        .ok_or_else(|| format!("cannot resolve namespace '{}'", ns_uri))
}

// ------------------------------------------------------------------
//  Recursive subtree walk
// ------------------------------------------------------------------

/// Walk one XML element, appending complete XPath branches.
fn walk_element(
    ctx: &Context,
    elem: &XmlElem,
    parent_xpath: &str,
    branches: &mut Vec<String>,
) -> Result<(), String> {
    let ns_uri = elem
        .namespace
        .as_deref()
        .ok_or_else(|| format!("no namespace for element '{}'", elem.local_name))?;
    let mod_name = resolve_ns(ctx, ns_uri)?;

    let mut current = format!("{}/{}:{}", parent_xpath, mod_name, elem.local_name);

    // Classify children into content-match vs. selection/containment
    let mut content_matches: Vec<(String, String)> = Vec::new();
    let mut selection_children: Vec<&XmlElem> = Vec::new();

    for child in &elem.children {
        let has_children = !child.children.is_empty();
        let has_text = child.text.as_ref().is_some_and(|t| !t.trim().is_empty());

        if has_text && !has_children {
            // Content match → becomes a predicate
            let child_ns = child
                .namespace
                .as_deref()
                .ok_or_else(|| format!("no namespace for '{}'", child.local_name))?;
            let child_mod = resolve_ns(ctx, child_ns)?;
            let qname = format!("{}:{}", child_mod, child.local_name);
            let value = child.text.as_ref().unwrap().clone();
            content_matches.push((qname, value));
        } else {
            // Selection or containment node → recurse
            selection_children.push(child);
        }
    }

    // Append content-match predicates
    for (qname, value) in &content_matches {
        current = format!("{}[{}={}]", current, qname, escape_xpath_value(value));
    }

    // Terminal or recurse
    if selection_children.is_empty() {
        branches.push(current);
    } else {
        for child in selection_children {
            walk_element(ctx, child, &current, branches)?;
        }
    }

    Ok(())
}

// ------------------------------------------------------------------
//  Public API
// ------------------------------------------------------------------

/// Normalize a YANG Push subtree filter (XML) to XPath expression(s).
///
/// # Arguments
///
/// * `ctx` — libyang context with all required YANG modules loaded.
/// * `subtree_xml` — the subtree filter as a well-formed XML string.
///
/// # Returns
///
/// One or more absolute XPath expressions joined by ` | `.
/// Each segment carries the full YANG module-name prefix.
///
/// # Errors
///
/// Returns `Err` if the XML cannot be parsed, a namespace cannot be
/// resolved, or the filter produces zero branches.
pub fn normalize_subtree(ctx: &Context, subtree_xml: &str) -> Result<String, String> {
    let tree = parse_xml_to_tree(subtree_xml)?;

    // Auto-strip <filter> wrapper
    let top_elements = if tree.len() == 1 && tree[0].local_name == "filter" {
        tree[0].children.clone()
    } else {
        tree
    };

    if top_elements.is_empty() {
        return Err("no data elements in subtree filter".into());
    }

    let mut branches = Vec::new();
    for elem in &top_elements {
        walk_element(ctx, elem, "", &mut branches)?;
    }

    // Deduplicate identical branches (preserving first-seen order)
    let mut seen = BTreeSet::new();
    branches.retain(|b| seen.insert(b.clone()));

    if branches.is_empty() {
        return Err("subtree filter produced no branches".into());
    }

    Ok(branches.join(" | "))
}
