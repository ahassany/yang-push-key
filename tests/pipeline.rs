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

//! Pipeline integration tests: end-to-end Phase 1 -> Phase 2 -> Phase 3.
//!
//! These tests verify that the three phases compose correctly when
//! chained together, using realistic subtree filter inputs.

mod common;

use common::{create_ctx, parse_data};
use yang_push_key::{derive_templates, normalize_subtree, produce_message_key};

// =====================================================================
//  Phase 1 -> Phase 2
// =====================================================================

#[test]
fn pipeline_p1_to_p2_pinned_key_and_container() {
    let ctx = create_ctx(&[
        ("ietf-ip", &[]),
        ("ietf-interfaces", &[]),
        ("ietf-system", &[]),
    ]);
    let filter_xml = include_str!("../assets/testdata/p1_pipeline.xml");

    let xpath = normalize_subtree(&ctx, filter_xml).expect("Phase 1 failed");
    let result = derive_templates(&ctx, &xpath).expect("Phase 2 failed");

    assert_eq!(result.branches.len(), 2);
    // Branch 0: interface mtu with name='eth0' pinned -> 0 extractions
    assert!(result.branches[0].key_template.contains("[name='eth0']"));
    assert_eq!(result.branches[0].extractions.len(), 0);
    // Branch 1: container clock -> no list keys
    assert_eq!(result.branches[1].key_template, "/ietf-system:system/clock");
    assert_eq!(result.branches[1].extractions.len(), 0);
}

#[test]
fn pipeline_p1_to_p2_outer_pinned_inner_open() {
    let ctx = create_ctx(&[("example-network", &[])]);
    let filter_xml = include_str!("../assets/testdata/p1_content_nested.xml");

    let xpath = normalize_subtree(&ctx, filter_xml).expect("Phase 1 failed");
    let result = derive_templates(&ctx, &xpath).expect("Phase 2 failed");

    assert_eq!(result.branches.len(), 1);
    // Outer list pinned by content match, inner list open -> 1 extraction
    assert!(result.branches[0].key_template.contains("[name='default']"));
    assert_eq!(result.branches[0].extractions.len(), 1);
    assert_eq!(result.branches[0].extractions[0].key_leaf_name, "id");
}

// =====================================================================
//  Phase 1 -> Phase 2 -> Phase 3 (full end-to-end)
// =====================================================================

#[test]
fn pipeline_p1_to_p2_to_p3_full_roundtrip() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let filter_xml = include_str!("../assets/testdata/p1_simple.xml");
    let data_xml = include_str!("../assets/testdata/if_single.xml");
    let expected_key = include_str!("../assets/testdata/expected/p1_to_p2_to_p3.key");

    let xpath = normalize_subtree(&ctx, filter_xml).expect("Phase 1 failed");
    let derivation = derive_templates(&ctx, &xpath).expect("Phase 2 failed");
    let data = parse_data(&ctx, data_xml);
    let result =
        produce_message_key(&derivation, &data, "router-nyc-01", "1042").expect("Phase 3 failed");

    assert_eq!(result.message_key, expected_key);
}

#[test]
fn pipeline_p1_to_p2_to_p3_nested_with_content_match() {
    let ctx = create_ctx(&[("example-network", &[])]);
    let filter_xml = include_str!("../assets/testdata/p1_content_nested.xml");
    let data_xml = include_str!("../assets/testdata/ni_single.xml");

    let xpath = normalize_subtree(&ctx, filter_xml).expect("Phase 1 failed");
    let derivation = derive_templates(&ctx, &xpath).expect("Phase 2 failed");
    let data = parse_data(&ctx, data_xml);
    let result =
        produce_message_key(&derivation, &data, "switch-dc-12", "7500").expect("Phase 3 failed");

    assert_eq!(result.key.node_name, "switch-dc-12");
    assert_eq!(result.key.subscription_id, "7500");
    assert!(result.key.xpaths[0].contains("[name='default']"));
    assert!(result.key.xpaths[0].contains("[id='eth0']"));
}
