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

//! Phase 1 integration tests: subtree filter -> XPath normalization.
//!
//! Each test follows the **Arrange–Act–Assert** pattern:
//! - **Arrange**: create a YANG context and load the input XML + expected XPath.
//! - **Act**: call [`normalize_subtree`].
//! - **Assert**: compare the produced XPath against the expected file.

mod common;

use common::create_ctx;
use yang_push_key::normalize_subtree;

// =====================================================================
//  Simple paths
// =====================================================================

#[test]
fn p1_01_simple_single_path() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let input = include_str!("../assets/testdata/p1_simple.xml");
    let expected = include_str!("../assets/testdata/expected/p1_simple.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

#[test]
fn p1_02_content_match_single_key() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let input = include_str!("../assets/testdata/p1_content.xml");
    let expected = include_str!("../assets/testdata/expected/p1_content.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

#[test]
fn p1_03_filter_wrapper_auto_stripped() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let input = include_str!("../assets/testdata/p1_filter.xml");
    let expected = include_str!("../assets/testdata/expected/p1_filter.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

#[test]
fn p1_04_multiple_top_level_elements() {
    let ctx = create_ctx(&[("ietf-interfaces", &[]), ("example-vlans", &[])]);
    let input = include_str!("../assets/testdata/p1_multi_top.xml");
    let expected = include_str!("../assets/testdata/expected/p1_multi_top.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

// =====================================================================
//  Multiple selection leaves
// =====================================================================

#[test]
fn p1_05_multiple_leaves_produce_union() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let input = include_str!("../assets/testdata/p1_multi_leaves.xml");
    let expected = include_str!("../assets/testdata/expected/p1_multi_leaves.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

#[test]
fn p1_17_three_leaves_produce_three_branches() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let input = include_str!("../assets/testdata/p1_three_leaves.xml");
    let expected = include_str!("../assets/testdata/expected/p1_three_leaves.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

// =====================================================================
//  Deep nesting
// =====================================================================

#[test]
fn p1_06_deep_nesting_three_levels() {
    let ctx = create_ctx(&[("example-network", &[])]);
    let input = include_str!("../assets/testdata/p1_deep.xml");
    let expected = include_str!("../assets/testdata/expected/p1_deep.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

// =====================================================================
//  Content match variants
// =====================================================================

#[test]
fn p1_07_content_match_pins_outer_list() {
    let ctx = create_ctx(&[("example-network", &[])]);
    let input = include_str!("../assets/testdata/p1_content_nested.xml");
    let expected = include_str!("../assets/testdata/expected/p1_content_nested.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

#[test]
fn p1_10_composite_key_content_match() {
    let ctx = create_ctx(&[("example-routes", &[])]);
    let input = include_str!("../assets/testdata/p1_composite.xml");
    let expected = include_str!("../assets/testdata/expected/p1_composite.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

#[test]
fn p1_12_content_match_with_multiple_selections() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let input = include_str!("../assets/testdata/p1_content_multi_sel.xml");
    let expected = include_str!("../assets/testdata/expected/p1_content_multi_sel.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

#[test]
fn p1_15_content_match_at_both_nesting_levels() {
    let ctx = create_ctx(&[("example-network", &[])]);
    let input = include_str!("../assets/testdata/p1_content_both_levels.xml");
    let expected = include_str!("../assets/testdata/expected/p1_content_both_levels.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

#[test]
fn p1_19_dns_server_nested_content_match() {
    let ctx = create_ctx(&[("ietf-system", &[])]);
    let input = include_str!("../assets/testdata/p1_dns_server.xml");
    let expected = include_str!("../assets/testdata/expected/p1_dns_server.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

#[test]
fn p1_21_content_match_value_with_slash() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let input = include_str!("../assets/testdata/p1_slash_value.xml");
    let expected = include_str!("../assets/testdata/expected/p1_slash_value.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

// =====================================================================
//  Quoting and escaping
// =====================================================================

#[test]
fn p1_08_value_with_single_quote_uses_double_quotes() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let input = include_str!("../assets/testdata/p1_quote.xml");
    let expected = include_str!("../assets/testdata/expected/p1_quote.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

// =====================================================================
//  Containers
// =====================================================================

#[test]
fn p1_11_container_only() {
    let ctx = create_ctx(&[("ietf-system", &[])]);
    let input = include_str!("../assets/testdata/p1_container.xml");
    let expected = include_str!("../assets/testdata/expected/p1_container.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

#[test]
fn p1_13_container_selecting_specific_leaf() {
    let ctx = create_ctx(&[("ietf-system", &[])]);
    let input = include_str!("../assets/testdata/p1_container_leaf.xml");
    let expected = include_str!("../assets/testdata/expected/p1_container_leaf.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

#[test]
fn p1_16_entire_top_level_container() {
    let ctx = create_ctx(&[("ietf-system", &[])]);
    let input = include_str!("../assets/testdata/p1_entire_container.xml");
    let expected = include_str!("../assets/testdata/expected/p1_entire_container.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

// =====================================================================
//  Edge cases
// =====================================================================

#[test]
fn p1_09_duplicate_branches_are_deduplicated() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let input = include_str!("../assets/testdata/p1_dedup.xml");
    let expected = include_str!("../assets/testdata/expected/p1_dedup.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

#[test]
fn p1_14_whitespace_only_text_is_not_content_match() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let input = include_str!("../assets/testdata/p1_whitespace.xml");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert!(
        !xpath.contains('['),
        "whitespace text must not produce a predicate"
    );
    assert!(xpath.contains("/ietf-interfaces:oper-status"));
}

// =====================================================================
//  Multi-module filters
// =====================================================================

#[test]
fn p1_18_two_modules_in_filter() {
    let ctx = create_ctx(&[("ietf-interfaces", &[]), ("ietf-system", &[])]);
    let input = include_str!("../assets/testdata/p1_multi_modules.xml");
    let expected = include_str!("../assets/testdata/expected/p1_multi_modules.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

#[test]
fn p1_20_sibling_selection_leaves() {
    let ctx = create_ctx(&[("ietf-system", &[])]);
    let input = include_str!("../assets/testdata/p1_sibling_leaves.xml");
    let expected = include_str!("../assets/testdata/expected/p1_sibling_leaves.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}

#[test]
fn p1_22_mixed_leaf_and_container_at_same_level() {
    let ctx = create_ctx(&[("ietf-system", &[])]);
    let input = include_str!("../assets/testdata/p1_mixed_leaf_container.xml");
    let expected = include_str!("../assets/testdata/expected/p1_mixed_leaf_container.xpath");

    let xpath = normalize_subtree(&ctx, input).expect("normalization failed");

    assert_eq!(xpath, expected);
}
