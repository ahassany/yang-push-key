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

//! Phase 3 integration tests: data tree -> message broker key (line-delimited).
//!
//! Each test follows the **Arrange-Act-Assert** pattern:
//! - **Arrange**: create a YANG context, derive templates (Phase 2),
//!   parse test XML data, and load the expected message key.
//! - **Act**: call [`produce_message_key`].
//! - **Assert**: compare the produced key against the expected file.

mod common;

use common::{create_ctx, parse_data};
use yang_push_key::{derive_templates, produce_message_key};

// =====================================================================
//  Single-instance notifications
// =====================================================================

#[test]
fn p3_01_single_list_instance() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let derivation =
        derive_templates(&ctx, "/ietf-interfaces:interfaces/interface").expect("derivation failed");
    let data = parse_data(&ctx, include_str!("../assets/testdata/if_single.xml"));
    let expected = include_str!("../assets/testdata/expected/p3_single_instance.key");

    let result = produce_message_key(&derivation, &data, "router-nyc-01", "1042")
        .expect("key production failed");

    assert_eq!(result.message_key, expected);
}

#[test]
fn p3_03_nested_list_instance() {
    let ctx = create_ctx(&[("example-network", &[])]);
    let derivation = derive_templates(
        &ctx,
        "/example-network:network-instances/network-instance/interface",
    )
    .expect("derivation failed");
    let data = parse_data(&ctx, include_str!("../assets/testdata/ni_single.xml"));
    let expected = include_str!("../assets/testdata/expected/p3_nested_list.key");

    let result = produce_message_key(&derivation, &data, "switch-dc-12", "7500")
        .expect("key production failed");

    assert_eq!(result.message_key, expected);
}

#[test]
fn p3_05_composite_key_extraction() {
    let ctx = create_ctx(&[("example-routes", &[])]);
    let derivation =
        derive_templates(&ctx, "/example-routes:routes/route").expect("derivation failed");
    let data = parse_data(&ctx, include_str!("../assets/testdata/routes_single.xml"));
    let expected = include_str!("../assets/testdata/expected/p3_composite_key.key");

    let result = produce_message_key(&derivation, &data, "router-west-05", "3001")
        .expect("key production failed");

    assert_eq!(result.message_key, expected);
}

#[test]
fn p3_07_leaf_inside_list() {
    let ctx = create_ctx(&[("ietf-ip", &[]), ("ietf-interfaces", &[])]);
    let derivation = derive_templates(
        &ctx,
        "/ietf-interfaces:interfaces/interface/ietf-ip:ipv4/mtu",
    )
    .expect("derivation failed");
    let data = parse_data(&ctx, include_str!("../assets/testdata/if_leaf.xml"));
    let expected = include_str!("../assets/testdata/expected/p3_leaf_in_list.key");

    let result = produce_message_key(&derivation, &data, "router-nyc-01", "1043")
        .expect("key production failed");

    assert_eq!(result.message_key, expected);
}

// =====================================================================
//  Container (no list instances)
// =====================================================================

#[test]
fn p3_04_container_produces_fixed_key() {
    let ctx = create_ctx(&[("ietf-system", &[])]);
    let derivation =
        derive_templates(&ctx, "/ietf-system:system/clock").expect("derivation failed");
    let data = parse_data(&ctx, include_str!("../assets/testdata/sys_clock.xml"));
    let expected = include_str!("../assets/testdata/expected/p3_container.key");

    let result = produce_message_key(&derivation, &data, "switch-lab-02", "2001")
        .expect("key production failed");

    assert_eq!(result.message_key, expected);
}

// =====================================================================
//  Multi-instance notifications (sorted + concatenated)
// =====================================================================

#[test]
fn p3_02_multiple_instances_sorted() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let derivation =
        derive_templates(&ctx, "/ietf-interfaces:interfaces/interface").expect("derivation failed");
    let data = parse_data(&ctx, include_str!("../assets/testdata/if_multi.xml"));
    let expected = include_str!("../assets/testdata/expected/p3_multiple_instances.key");

    let result = produce_message_key(&derivation, &data, "router-nyc-01", "1042")
        .expect("key production failed");

    assert_eq!(result.message_key, expected);
    assert_eq!(result.key.xpaths.len(), 2);
    assert!(result.key.xpaths[0].contains("[name='eth0']"));
    assert!(result.key.xpaths[1].contains("[name='eth1']"));
}

#[test]
fn p3_08_nested_multiple_inner_instances() {
    let ctx = create_ctx(&[("example-network", &[])]);
    let derivation = derive_templates(
        &ctx,
        "/example-network:network-instances/network-instance/interface",
    )
    .expect("derivation failed");
    let data = parse_data(&ctx, include_str!("../assets/testdata/ni_multi.xml"));

    let result = produce_message_key(&derivation, &data, "switch-dc-12", "7500")
        .expect("key production failed");

    assert_eq!(result.key.xpaths.len(), 2);
    assert!(result.key.xpaths[0].contains("[id='eth0']"));
    assert!(result.key.xpaths[1].contains("[id='eth1']"));
}

// =====================================================================
//  Leaf-list runtime
// =====================================================================

#[test]
fn p3_09_leaf_list_values() {
    let ctx = create_ctx(&[("ietf-system", &[])]);
    let derivation = derive_templates(&ctx, "/ietf-system:system/dns-resolver/search")
        .expect("derivation failed");
    let data = parse_data(&ctx, include_str!("../assets/testdata/sys_dns.xml"));

    let result = produce_message_key(&derivation, &data, "router-east-03", "6000")
        .expect("key production failed");

    assert_eq!(result.key.xpaths.len(), 2);
    assert!(result.key.xpaths[0].contains("corp.example.com"));
}

// =====================================================================
//  Cross-device isolation
// =====================================================================

#[test]
fn p3_06_same_data_different_nodes_produce_distinct_keys() {
    let ctx = create_ctx(&[("ietf-interfaces", &[])]);
    let derivation =
        derive_templates(&ctx, "/ietf-interfaces:interfaces/interface").expect("derivation failed");
    let data = parse_data(&ctx, include_str!("../assets/testdata/if_single.xml"));

    let key_a = produce_message_key(&derivation, &data, "router-nyc-01", "1042")
        .expect("key production failed");
    let key_b = produce_message_key(&derivation, &data, "router-lon-01", "1042")
        .expect("key production failed");

    assert_ne!(key_a.message_key, key_b.message_key);
    assert_eq!(key_a.key.node_name, "router-nyc-01");
    assert_eq!(key_b.key.node_name, "router-lon-01");
    // Same xpaths, different node_name
    assert_eq!(key_a.key.xpaths, key_b.key.xpaths);
}
