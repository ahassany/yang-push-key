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

//! Phase 2 integration tests: XPath + schema -> key template derivation.
//!
//! Each test follows the **Arrange–Act–Assert** pattern:
//! - **Arrange**: create a YANG context; define the subscription XPath
//!   and the expected key template.
//! - **Act**: call [`derive_templates`].
//! - **Assert**: compare the template, extraction count, and target type.

mod common;

use common::create_ctx;
use yang_push_key::derive_templates;
use yang_push_key::types::TargetType;

// =====================================================================
//  Bare XPaths (no predicates)
// =====================================================================

#[test]
fn p2_01_simple_list_single_key() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface";
    let expected_template = include_str!("../assets/testdata/expected/p2_simple_list.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches.len(), 1);
    assert_eq!(result.branches[0].key_template, expected_template);
    assert_eq!(result.branches[0].target_type, TargetType::List);
    assert_eq!(result.branches[0].extractions.len(), 1);
    assert_eq!(result.branches[0].extractions[0].key_leaf_name, "name");
}

#[test]
fn p2_02_redundant_prefix_normalized() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/ietf-interfaces:interface";
    let expected_template = include_str!("../assets/testdata/expected/p2_simple_list.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches[0].key_template, expected_template);
}

#[test]
fn p2_03_composite_key_two_leaves() {
    let ctx = create_ctx(&[("example-routes", &[])]);
    let xpath = "/example-routes:routes/route";
    let expected_template = include_str!("../assets/testdata/expected/p2_composite_key.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches[0].key_template, expected_template);
    assert_eq!(result.branches[0].extractions.len(), 2);
    assert_eq!(
        result.branches[0].extractions[0].key_leaf_name,
        "destination-prefix"
    );
    assert_eq!(result.branches[0].extractions[1].key_leaf_name, "next-hop");
}

#[test]
fn p2_04_nested_lists() {
    let ctx = create_ctx(&[("example-network", &[])]);
    let xpath = "/example-network:network-instances/network-instance/interface";
    let expected_template = include_str!("../assets/testdata/expected/p2_nested_lists.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches[0].key_template, expected_template);
    assert_eq!(result.branches[0].extractions.len(), 2);
    assert_eq!(
        result.branches[0].extractions[0].list_name,
        "network-instance"
    );
    assert_eq!(result.branches[0].extractions[1].list_name, "interface");
}

#[test]
fn p2_05_container_only_no_placeholders() {
    let ctx = create_ctx(&[("ietf-system", &[])]);
    let xpath = "/ietf-system:system/clock";
    let expected_template = include_str!("../assets/testdata/expected/p2_container_only.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches[0].key_template, expected_template);
    assert_eq!(result.branches[0].target_type, TargetType::Container);
    assert_eq!(result.branches[0].extractions.len(), 0);
}

#[test]
fn p2_06_leaf_inside_list() {
    let ctx = create_ctx(&[("ietf-ip", &[]), ("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface/ietf-ip:ipv4/mtu";
    let expected_template = include_str!("../assets/testdata/expected/p2_leaf_in_list.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches[0].key_template, expected_template);
    assert_eq!(result.branches[0].target_type, TargetType::Leaf);
    assert_eq!(result.branches[0].extractions.len(), 1);
}

#[test]
fn p2_07_leaf_list_target() {
    let ctx = create_ctx(&[("ietf-system", &[])]);
    let xpath = "/ietf-system:system/dns-resolver/search";
    let expected_template = include_str!("../assets/testdata/expected/p2_leaf_list.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches[0].key_template, expected_template);
    assert_eq!(result.branches[0].target_type, TargetType::LeafList);
    assert_eq!(result.branches[0].extractions[0].key_leaf_name, ".");
}

#[test]
fn p2_08_deep_nested_composite_keys() {
    let ctx = create_ctx(&[("example-acl", &[])]);
    let xpath = "/example-acl:access-lists/access-list/access-list-entry";
    let expected_template = include_str!("../assets/testdata/expected/p2_deep_composite.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches[0].key_template, expected_template);
    assert_eq!(result.branches[0].extractions.len(), 3);
}

#[test]
fn p2_10_three_levels_of_nesting() {
    let ctx = create_ctx(&[("example-deep", &[])]);
    let xpath = "/example-deep:root/level1/level2/level3";
    let expected_template = include_str!("../assets/testdata/expected/p2_three_level.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches[0].key_template, expected_template);
    assert_eq!(result.branches[0].extractions.len(), 3);
}

#[test]
fn p2_11_leaf_inside_deep_composite() {
    let ctx = create_ctx(&[("example-acl", &[])]);
    let xpath = "/example-acl:access-lists/access-list/access-list-entry/action";
    let expected_template =
        include_str!("../assets/testdata/expected/p2_leaf_deep_composite.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches[0].key_template, expected_template);
    assert_eq!(result.branches[0].target_type, TargetType::Leaf);
    assert_eq!(result.branches[0].extractions.len(), 3);
}

// =====================================================================
//  Union XPaths
// =====================================================================

#[test]
fn p2_09_union_two_list_branches() {
    let ctx = create_ctx(&[("ietf-interfaces", &[]), ("example-vlans", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface | /example-vlans:vlans/vlan";

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches.len(), 2);
    assert!(
        result.branches[0]
            .key_template
            .contains("interface[name='%s']")
    );
    assert!(
        result.branches[1]
            .key_template
            .contains("vlan[vlan-id='%s']")
    );
}

#[test]
fn p2_10b_union_three_branches_mixed_types() {
    let ctx = create_ctx(&[("ietf-interfaces", &[]), ("ietf-system", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface \
                 | /ietf-system:system/clock \
                 | /ietf-system:system/dns-resolver/search";

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches.len(), 3);
    assert_eq!(result.branches[0].target_type, TargetType::List);
    assert_eq!(result.branches[1].target_type, TargetType::Container);
    assert_eq!(result.branches[2].target_type, TargetType::LeafList);
}

#[test]
fn p2_21_union_one_concrete_one_open() {
    let ctx = create_ctx(&[("ietf-interfaces", &[]), ("example-vlans", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface[name='eth0'] \
                 | /example-vlans:vlans/vlan";

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches.len(), 2);
    assert_eq!(result.branches[0].extractions.len(), 0); // pinned
    assert_eq!(result.branches[1].extractions.len(), 1); // open
}

// =====================================================================
//  Concrete / predicated XPaths
// =====================================================================

#[test]
fn p2_13_fully_concrete_single_key() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface[name='eth0']";
    let expected_template = include_str!("../assets/testdata/expected/p2_concrete_single.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches[0].key_template, expected_template);
    assert_eq!(result.branches[0].extractions.len(), 0);
}

#[test]
fn p2_14_fully_concrete_nested() {
    let ctx = create_ctx(&[("example-network", &[])]);
    let xpath = "/example-network:network-instances\
                 /network-instance[name='default']\
                 /interface[id='eth0']";

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert!(result.branches[0].key_template.contains("[name='default']"));
    assert!(result.branches[0].key_template.contains("[id='eth0']"));
    assert_eq!(result.branches[0].extractions.len(), 0);
}

#[test]
fn p2_15_partial_concrete_outer_pinned_inner_open() {
    let ctx = create_ctx(&[("example-network", &[])]);
    let xpath = "/example-network:network-instances\
                 /network-instance[name='mgmt']/interface";
    let expected_template =
        include_str!("../assets/testdata/expected/p2_concrete_partial.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches[0].key_template, expected_template);
    assert_eq!(result.branches[0].extractions.len(), 1);
    assert_eq!(result.branches[0].extractions[0].key_leaf_name, "id");
}

#[test]
fn p2_16_fully_concrete_composite_key() {
    let ctx = create_ctx(&[("example-routes", &[])]);
    let xpath = "/example-routes:routes/route\
                 [destination-prefix='10.0.0.0/8'][next-hop='192.168.1.1']";

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert!(
        result.branches[0]
            .key_template
            .contains("[destination-prefix='10.0.0.0/8']")
    );
    assert!(
        result.branches[0]
            .key_template
            .contains("[next-hop='192.168.1.1']")
    );
    assert_eq!(result.branches[0].extractions.len(), 0);
}

#[test]
fn p2_17_partial_composite_one_pinned_one_open() {
    let ctx = create_ctx(&[("example-routes", &[])]);
    let xpath = "/example-routes:routes/route[destination-prefix='10.0.0.0/8']";
    let expected_template =
        include_str!("../assets/testdata/expected/p2_concrete_composite_partial.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches[0].key_template, expected_template);
    assert_eq!(result.branches[0].extractions.len(), 1);
    assert_eq!(result.branches[0].extractions[0].key_leaf_name, "next-hop");
}

#[test]
fn p2_18_double_quoted_value_normalized() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let xpath = r#"/ietf-interfaces:interfaces/interface[name="eth0"]"#;
    let expected_template = include_str!("../assets/testdata/expected/p2_double_quoted.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches[0].key_template, expected_template);
    assert_eq!(result.branches[0].extractions.len(), 0);
}

#[test]
fn p2_19_positional_predicate_treated_as_open() {
    let ctx = create_ctx(&[("example-network", &[])]);
    let xpath = "/example-network:network-instances/network-instance[1]/interface";

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert!(result.branches[0].key_template.contains("[name='%s']"));
    assert_eq!(result.branches[0].extractions.len(), 2);
}

#[test]
fn p2_20_value_containing_single_quote() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let xpath = r#"/ietf-interfaces:interfaces/interface[name="O'Brien"]"#;

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert!(result.branches[0].key_template.contains(r#""O'Brien""#));
    assert_eq!(result.branches[0].extractions.len(), 0);
}

#[test]
fn p2_22_concrete_leaf_inside_concrete_list() {
    let ctx = create_ctx(&[("ietf-ip", &[]), ("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface[name='eth0']/ietf-ip:ipv4/mtu";
    let expected_template = include_str!("../assets/testdata/expected/p2_concrete_leaf.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches[0].key_template, expected_template);
    assert_eq!(result.branches[0].target_type, TargetType::Leaf);
    assert_eq!(result.branches[0].extractions.len(), 0);
}

#[test]
fn p2_23_concrete_leaf_list_value() {
    let ctx = create_ctx(&[("ietf-system", &[])]);
    let xpath = "/ietf-system:system/dns-resolver/search[.='example.com']";

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert!(
        result.branches[0]
            .key_template
            .contains("[.='example.com']")
    );
    assert_eq!(result.branches[0].target_type, TargetType::LeafList);
    assert_eq!(result.branches[0].extractions.len(), 0);
}

#[test]
fn p2_24_deep_mixed_outer_pinned_inner_open() {
    let ctx = create_ctx(&[("example-acl", &[])]);
    let xpath = "/example-acl:access-lists\
                 /access-list[name='fw-in'][type='ipv4']\
                 /access-list-entry";
    let expected_template = include_str!("../assets/testdata/expected/p2_deep_mixed.template");

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(result.branches[0].key_template, expected_template);
    assert_eq!(result.branches[0].extractions.len(), 1);
    assert_eq!(
        result.branches[0].extractions[0].key_leaf_name,
        "sequence-id"
    );
}

#[test]
fn p2_25_module_prefixed_predicate_key() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface[ietf-interfaces:name='eth0']";

    let result = derive_templates(&ctx, xpath).expect("derivation failed");

    assert_eq!(
        result.branches[0].key_template,
        "/ietf-interfaces:interfaces/interface[name='eth0']"
    );
    assert_eq!(result.branches[0].extractions.len(), 0);
}
