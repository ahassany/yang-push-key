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

//! Topic name derivation integration tests.
//!
//! Each test follows the **Arrange-Act-Assert** pattern:
//! - **Arrange**: create a YANG context, define the subscription XPath,
//!   and load the expected topic name.
//! - **Act**: call `derive_templates` then `derive_topic_names`.
//! - **Assert**: compare the produced topic name against the expected file.

mod common;

use common::create_ctx;
use yang_push_key::{derive_templates, derive_topic_names, TopicConfig};

// =====================================================================
//  Simple list and leaf targets
// =====================================================================

#[test]
fn topic_simple_list() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface";
    let expected = include_str!("../assets/testdata/expected/topic_simple_list.topic");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

    assert_eq!(result.topic_names.len(), 1);
    assert_eq!(result.topic_names[0], expected);
}

#[test]
fn topic_leaf_in_list() {
    let ctx = create_ctx(&[("ietf-ip", &[]), ("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface/ietf-ip:ipv4/mtu";
    let expected = include_str!("../assets/testdata/expected/topic_leaf_in_list.topic");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

    assert_eq!(result.topic_names[0], expected);
}

#[test]
fn topic_oper_status() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface/oper-status";
    let expected = include_str!("../assets/testdata/expected/topic_oper_status.topic");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

    assert_eq!(result.topic_names[0], expected);
}

// =====================================================================
//  Containers and leaf-lists
// =====================================================================

#[test]
fn topic_container() {
    let ctx = create_ctx(&[("ietf-system", &[])]);
    let xpath = "/ietf-system:system/clock";
    let expected = include_str!("../assets/testdata/expected/topic_container.topic");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

    assert_eq!(result.topic_names[0], expected);
}

#[test]
fn topic_leaf_list() {
    let ctx = create_ctx(&[("ietf-system", &[])]);
    let xpath = "/ietf-system:system/dns-resolver/search";
    let expected = include_str!("../assets/testdata/expected/topic_leaf_list.topic");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

    assert_eq!(result.topic_names[0], expected);
}

#[test]
fn topic_dns_server() {
    let ctx = create_ctx(&[("ietf-system", &[])]);
    let xpath = "/ietf-system:system/dns-resolver/search";
    let expected = include_str!("../assets/testdata/expected/topic_dns_server.topic");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

    assert_eq!(result.topic_names[0], expected);
}

// =====================================================================
//  Nested lists
// =====================================================================

#[test]
fn topic_nested_list() {
    let ctx = create_ctx(&[("example-network", &[])]);
    let xpath = "/example-network:network-instances/network-instance/interface";
    let expected = include_str!("../assets/testdata/expected/topic_nested_list.topic");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

    assert_eq!(result.topic_names[0], expected);
}

#[test]
fn topic_nested_status() {
    let ctx = create_ctx(&[("example-network", &[])]);
    let xpath = "/example-network:network-instances/network-instance/interface/status";
    let expected = include_str!("../assets/testdata/expected/topic_nested_status.topic");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

    assert_eq!(result.topic_names[0], expected);
}

// =====================================================================
//  Composite keys and deep nesting
// =====================================================================

#[test]
fn topic_composite_key() {
    let ctx = create_ctx(&[("example-routes", &[])]);
    let xpath = "/example-routes:routes/route";
    let expected = include_str!("../assets/testdata/expected/topic_composite.topic");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

    assert_eq!(result.topic_names[0], expected);
}

#[test]
fn topic_three_level_nesting() {
    let ctx = create_ctx(&[("example-deep", &[])]);
    let xpath = "/example-deep:root/level1/level2/level3";
    let expected = include_str!("../assets/testdata/expected/topic_deep.topic");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

    assert_eq!(result.topic_names[0], expected);
}

#[test]
fn topic_acl_entry() {
    let ctx = create_ctx(&[("example-acl", &[])]);
    let xpath = "/example-acl:access-lists/access-list/access-list-entry";
    let expected = include_str!("../assets/testdata/expected/topic_acl_entry.topic");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

    assert_eq!(result.topic_names[0], expected);
}

#[test]
fn topic_acl_leaf() {
    let ctx = create_ctx(&[("example-acl", &[])]);
    let xpath = "/example-acl:access-lists/access-list/access-list-entry/action";
    let expected = include_str!("../assets/testdata/expected/topic_acl_action.topic");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

    assert_eq!(result.topic_names[0], expected);
}

#[test]
fn topic_vlan() {
    let ctx = create_ctx(&[("example-vlans", &[])]);
    let xpath = "/example-vlans:vlans/vlan";
    let expected = include_str!("../assets/testdata/expected/topic_vlan.topic");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

    assert_eq!(result.topic_names[0], expected);
}

// =====================================================================
//  Concrete XPaths produce the same topic as bare XPaths
// =====================================================================

#[test]
fn topic_concrete_single_key_matches_bare() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let bare = "/ietf-interfaces:interfaces/interface";
    let concrete = "/ietf-interfaces:interfaces/interface[name='eth0']";
    let expected = include_str!("../assets/testdata/expected/topic_concrete_single.topic");

    let d_bare = derive_templates(&ctx, bare).expect("derivation failed");
    let d_concrete = derive_templates(&ctx, concrete).expect("derivation failed");
    let t_bare = derive_topic_names(&ctx, &d_bare, &TopicConfig::default()).expect("topic failed");
    let t_concrete = derive_topic_names(&ctx, &d_concrete, &TopicConfig::default()).expect("topic failed");

    assert_eq!(t_bare.topic_names[0], expected);
    assert_eq!(t_concrete.topic_names[0], expected);
    assert_eq!(t_bare.topic_names[0], t_concrete.topic_names[0]);
}

#[test]
fn topic_concrete_leaf_matches_bare() {
    let ctx = create_ctx(&[("ietf-ip", &[]), ("ietf-interfaces", &[])]);
    let bare = "/ietf-interfaces:interfaces/interface/ietf-ip:ipv4/mtu";
    let concrete = "/ietf-interfaces:interfaces/interface[name='eth0']/ietf-ip:ipv4/mtu";
    let expected = include_str!("../assets/testdata/expected/topic_concrete_leaf.topic");

    let d_bare = derive_templates(&ctx, bare).expect("derivation failed");
    let d_concrete = derive_templates(&ctx, concrete).expect("derivation failed");
    let t_bare = derive_topic_names(&ctx, &d_bare, &TopicConfig::default()).expect("topic failed");
    let t_concrete = derive_topic_names(&ctx, &d_concrete, &TopicConfig::default()).expect("topic failed");

    assert_eq!(t_bare.topic_names[0], expected);
    assert_eq!(t_concrete.topic_names[0], expected);
}

// =====================================================================
//  Union XPaths produce multiple topic names
// =====================================================================

#[test]
fn topic_union_two_branches() {
    let ctx = create_ctx(&[("ietf-interfaces", &[]), ("example-vlans", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface | /example-vlans:vlans/vlan";

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

    assert_eq!(result.topic_names.len(), 2);
    assert_eq!(result.topic_names[0], "if-interfaces-interface");
    assert_eq!(result.topic_names[1], "vlan-vlans-vlan");
}

#[test]
fn topic_union_three_mixed() {
    let ctx = create_ctx(&[("ietf-interfaces", &[]), ("ietf-system", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface \
                 | /ietf-system:system/clock \
                 | /ietf-system:system/dns-resolver/search";

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

    assert_eq!(result.topic_names.len(), 3);
    assert_eq!(result.topic_names[0], "if-interfaces-interface");
    assert_eq!(result.topic_names[1], "sys-system-clock");
    assert_eq!(result.topic_names[2], "sys-system-dns-resolver-search");
}

// =====================================================================
//  Topic name character validity
// =====================================================================

#[test]
fn topic_names_contain_only_valid_kafka_chars() {
    let ctx = create_ctx(&[
        ("ietf-ip", &[]),
        ("ietf-interfaces", &[]),
        ("ietf-system", &[]),
        ("example-network", &[]),
        ("example-acl", &[]),
        ("example-deep", &[]),
    ]);

    let xpaths = [
        "/ietf-interfaces:interfaces/interface/ietf-ip:ipv4/mtu",
        "/ietf-system:system/dns-resolver/search",
        "/example-network:network-instances/network-instance/interface/status",
        "/example-acl:access-lists/access-list/access-list-entry/action",
        "/example-deep:root/level1/level2/level3",
    ];

    for xpath in &xpaths {
        let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
        let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default()).expect("topic derivation failed");

        for name in &result.topic_names {
            assert!(
                name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'),
                "Topic '{}' contains invalid character (from xpath: {})",
                name,
                xpath,
            );
            assert!(name.len() <= 249, "Topic '{}' exceeds Kafka limit", name);
        }
    }
}

// =====================================================================
//  Organization prefix
// =====================================================================

#[test]
fn topic_with_prefix() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface";
    let config = TopicConfig::new().with_prefix("netops");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &config).expect("topic derivation failed");

    assert_eq!(result.topic_names[0], "netops-if-interfaces-interface");
}

#[test]
fn topic_with_long_prefix() {
    let ctx = create_ctx(&[("ietf-ip", &[]), ("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface/ietf-ip:ipv4/mtu";
    let config = TopicConfig::new().with_prefix("acme-corp-platform-team");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &config).expect("topic derivation failed");

    assert_eq!(
        result.topic_names[0],
        "acme-corp-platform-team-if-interfaces-interface-ip-ipv4-mtu"
    );
}

#[test]
fn topic_with_empty_prefix_same_as_no_prefix() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface";

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let with_empty = derive_topic_names(
        &ctx, &derivation,
        &TopicConfig::new().with_prefix(""),
    ).expect("topic derivation failed");
    let without = derive_topic_names(
        &ctx, &derivation,
        &TopicConfig::default(),
    ).expect("topic derivation failed");

    assert_eq!(with_empty.topic_names, without.topic_names);
}

#[test]
fn topic_prefix_on_union_applies_to_all_branches() {
    let ctx = create_ctx(&[("ietf-interfaces", &[]), ("example-vlans", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface | /example-vlans:vlans/vlan";
    let config = TopicConfig::new().with_prefix("team-a");

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &config).expect("topic derivation failed");

    assert!(result.topic_names[0].starts_with("team-a-"));
    assert!(result.topic_names[1].starts_with("team-a-"));
}

// =====================================================================
//  Configurable max length
// =====================================================================

#[test]
fn topic_custom_max_length() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface/oper-status";
    let config = TopicConfig::new().with_max_length(30);

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &config).expect("topic derivation failed");

    // "if-interfaces-interface-oper-status" is 35 chars, exceeds 30 -> truncated
    assert!(result.topic_names[0].len() <= 30);
    // Should end with 8-char hash
    let last_dash = result.topic_names[0].rfind('-').unwrap();
    let suffix = &result.topic_names[0][last_dash + 1..];
    assert_eq!(suffix.len(), 8);
}

#[test]
fn topic_prefix_plus_max_length_interaction() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface/oper-status";
    let config = TopicConfig::new()
        .with_prefix("ops")
        .with_max_length(40);

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &config).expect("topic derivation failed");

    // Total must be <= 40
    assert!(result.topic_names[0].len() <= 40);
    assert!(result.topic_names[0].starts_with("ops-"));
}

#[test]
fn topic_prefix_too_long_for_budget_returns_error() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface";
    // prefix "x".repeat(250) + "-" = 251 chars, budget = 255 - 251 = 4, less than HASH_SUFFIX_LEN + 2 = 10
    let long_prefix: String = "x".repeat(250);
    let config = TopicConfig::new()
        .with_prefix(long_prefix)
        .with_max_length(255);

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &config);

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("too long"));
}

#[test]
fn topic_within_default_max_not_truncated() {
    let ctx = create_ctx(&[("ietf-ip", &[]), ("ietf-interfaces", &[])]);
    let xpath = "/ietf-interfaces:interfaces/interface/ietf-ip:ipv4/mtu";

    let derivation = derive_templates(&ctx, xpath).expect("derivation failed");
    let result = derive_topic_names(&ctx, &derivation, &TopicConfig::default())
        .expect("topic derivation failed");

    // "if-interfaces-interface-mtu" = 27 chars, well within 255
    assert_eq!(result.topic_names[0], "if-interfaces-interface-ip-ipv4-mtu");
    assert!(!result.topic_names[0].chars().any(|c| c.is_ascii_hexdigit() && c.is_ascii_lowercase()) 
        || result.topic_names[0].contains("interface")); // no hash appended
}
