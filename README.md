# yang-push-key

Derive unique Apache Kafka message keys from YANG Push on-change notifications
([RFC 8641](https://www.rfc-editor.org/rfc/rfc8641)).

## Problem Statement

YANG Push ([RFC 8641](https://www.rfc-editor.org/rfc/rfc8641)) allows network
devices to stream configuration and state changes as notifications. When these
notifications are published to an Apache Kafka compact topic, each message
needs a deterministic key so that Kafka's log compaction retains only the most
recent value for each logical piece of state.

The challenge is that YANG Push subscriptions can target any node in the schema
tree — a single leaf, a list entry, an entire container, or even a union of
paths — and the notification XML carries only the changed data, not a
ready-made key. The subscription can be specified either as an XPath expression
or as a subtree filter (RFC 6241 Section 6), and the same logical data path
can be expressed in many syntactic variants (redundant module prefixes,
double-quoted vs. single-quoted predicates, positional subscripts, etc.).

This tool solves the problem with a three-phase algorithm that normalizes these
variants, resolves the subscription against the YANG schema, and produces a
compact, deterministic JSON key at notification delivery time.

## Kafka Key Format

The output key is a compact JSON object with deterministic field ordering:

```json
{
  "node_name": "router-nyc-01",
  "subscription_id": "1042",
  "xpaths": [
    "/ietf-interfaces:interfaces/interface[name='eth0']"
  ]
}
```

| Field             | Type            | Description                                    |
|-------------------|-----------------|------------------------------------------------|
| `node_name`       | string          | Managed node identifier (hostname, FQDN, etc.) |
| `subscription_id` | string          | YANG Push subscription ID                      |
| `xpaths`          | array of string | Sorted, deduplicated concrete XPaths           |

Compact serialization (no whitespace) guarantees byte-identical keys for
identical inputs — the invariant Kafka log compaction depends on.

When a single notification carries multiple instances (e.g. two interfaces
changed at once), the `xpaths` array contains one entry per instance, sorted
lexicographically. This means one notification always produces exactly one
Kafka message, and the key is always deterministic regardless of the order the
instances appear in the XML.

## Algorithm

The algorithm has three phases. Phase 1 is only needed when the subscription
uses a subtree filter; XPath subscriptions skip directly to Phase 2. Phase 2
runs once at subscription creation time. Phase 3 runs for every notification.

```
  +-------------------+       +--------------+
  | Subtree Filter    |------>| Phase 1:     |---> Normalized XPath(s)
  | (if applicable)   |       | Normalize    |     (full-prefix style)
  +-------------------+       +--------------+           |
                                                         v
  +-------------------+       +--------------+     +---------------+
  | Subscription Path |------>|              |     | Key Templates |
  | (XPath, possibly  |       |  Phase 2:    |---->| (minimal-     |
  |  from Phase 1)    |       |  Schema      |     |  prefix, %s)  |
  +-------------------+       |  Resolution  |     +---------------+
  | YANG Schema Tree  |------>|              |     | Extraction    |
  +-------------------+       +--------------+     | Specs[]       |
                                                   +-------+-------+
                                                           |
                                                           v
  +-------------------+       +--------------+     +-----------------+
  | Parsed Data Tree  |------>|              |     | Kafka Key       |
  +-------------------+       |  Phase 3:    |---->| device/sub-id   |
  | Device Name       |------>|  Data Walk   |     | /concrete-keys  |
  | Subscription ID   |------>|              |     +-----------------+
  +-------------------+       +--------------+
```


### Phase 1: Subtree Filter Normalization

**Implementation:** [`src/phase1.rs`](src/phase1.rs) — `normalize_subtree()`

Converts an RFC 6241 subtree filter (XML) into one or more equivalent XPath
expressions joined by ` | `.

#### Element Classification (RFC 6241 Section 6.2)

Each child element in the filter is classified based on its content:

| Classification    | Has text content? | Has child elements? | Treatment                                 |
|-------------------|-------------------|---------------------|-------------------------------------------|
| **Content match** | yes               | no                  | Becomes an `[qname='value']` predicate    |
| **Selection**     | no                | no                  | Terminal node — ends a branch             |
| **Containment**   | no or whitespace  | yes                 | Intermediate step — recurse into children |

The algorithm walks the XML tree recursively. At each element it resolves the
XML namespace to a YANG module name (via libyang's `get_module_implemented_ns`),
builds the current path step as `module:local-name`, classifies children, and
either terminates the branch or recurses.

#### Output Prefix Convention

Phase 1 emits **full module-name prefixes** on every path segment:

```
/ietf-interfaces:interfaces/ietf-interfaces:interface
```

Phase 2 accepts this and normalizes to minimal-prefix style (see below).

#### Internal Steps

1. **Parse XML** into a lightweight tree (quick-xml 0.39).
2. **Strip `<filter>` wrapper** if present (the `<filter>` element itself is
   not a data node).
3. **Walk** each top-level data element recursively, classifying children.
4. **Build XPath branches**: content-match children become predicates on the
   parent step; selection children become terminal branches; containment
   children recurse.
5. **Deduplicate** identical branches (preserving first-seen order).
6. **Join** branches with ` | `.

### Phase 2: Key Template Derivation

**Implementation:** [`src/phase2.rs`](src/phase2.rs) — `derive_templates()`

Given a subscription XPath and a compiled YANG schema context, resolve each
union branch to its target schema node, walk the ancestor chain from root to
target, and build a key template.

#### Template Format

The key template uses **minimal-prefix** style: the YANG module name appears
only on the first segment or when the module changes. List key predicates use
`%s` as a printf-style placeholder for values that will be extracted at
runtime, and literal values for keys that are statically pinned in the
subscription.

Example template for `/ietf-interfaces:interfaces/interface`:

```
/ietf-interfaces:interfaces/interface[name='%s']
```

Example template for `/example-routes:routes/route[destination-prefix='10.0.0.0/8']`:

```
/example-routes:routes/route[destination-prefix='10.0.0.0/8'][next-hop='%s']
```

The first key is pinned (literal), the second is open (placeholder).

#### Internal Steps

1. **Split** the XPath on top-level `|` into individual branches
   ([`src/xpath.rs`](src/xpath.rs) — `split_union()`).
2. **Resolve** each branch to its target schema node via `find_xpath` /
   `find_path`.
3. **Walk ancestors** from target to root (skipping `choice`/`case` nodes),
   reverse to root-to-target order.
4. For each node, **emit a path segment** with minimal prefix.
5. At each **list** node, iterate the schema-defined key leaves (in schema
   order) and check the original XPath step for a matching predicate:
    - Predicate found with literal value -> emit `[key='value']` (pinned).
    - No predicate, or positional predicate `[N]` -> emit `[key='%s']` and
      record an `ExtractionSpec`.
6. At a **leaf-list** target, emit `[.='%s']` or a literal `[.='value']`.

#### Predicate Normalization

| Input form               | Template output                                          |
|--------------------------|----------------------------------------------------------|
| No predicate (bare path) | `[key='%s']` + extraction                                |
| `[key='value']`          | `[key='value']` (literal)                                |
| `[mod:key='value']`      | `[key='value']` (module prefix stripped)                 |
| `[key="value"]`          | `[key='value']` (double-quote normalized to single)      |
| `[N]` (positional)       | `[key='%s']` (treated as open — positional does not pin) |

### Phase 3: Runtime Kafka Key Production

**Implementation:** [`src/phase3.rs`](src/phase3.rs) — `produce_kafka_key()`

Given a parsed notification data tree and the Phase 2 derivation result, walk
the data tree, match instances to branch templates, extract key leaf values,
fill placeholders, and compose the final Kafka key.

#### Internal Steps

1. **Walk** the data tree (depth-first, sibling-inclusive).
2. For each data node, check if its schema path (with predicates stripped)
   matches any branch template.
3. On match, **fill the template**: for each `%s` placeholder, walk up from
   the data node to find the ancestor list matching the extraction spec, then
   read the key leaf child's canonical value.
4. **Collect** all filled templates into a list.
5. **Deduplicate and sort** lexicographically.
6. **Build the `KafkaKey` struct** with `node_name`, `subscription_id`, and
   the sorted `xpaths` array.
7. **Serialize** to compact JSON via `serde_json::to_string`.

### Supporting Utilities

**Implementation:** [`src/xpath.rs`](src/xpath.rs)

- `split_union()` — splits XPath on `|`, respecting brackets/quotes/parens.
- `parse_predicates()` — parses `[key='value']` pairs from a predicate string.
- `parse_xpath_steps()` — splits an absolute XPath into individual steps with
  parsed predicates.
- `escape_xpath_value()` — wraps a value in the appropriate quote style.
- `strip_predicates()` — removes all `[...]` from a path.

### Shared Types

**Implementation:** [`src/types.rs`](src/types.rs)

- `TargetType` — enum: `Container`, `List`, `Leaf`, `LeafList`.
- `ExtractionSpec` — describes one `%s` placeholder (key leaf name, owning
  list module/name).
- `BranchTemplate` — template string + extractions + target type for one
  union branch.
- `DerivationResult` — complete Phase 2 output (all branches).
- `KafkaKey` — the serializable JSON key struct.
- `KafkaKeyResult` — compact JSON string + structured `KafkaKey`.

## Special Case Handling

### Content Match vs. Whitespace (Phase 1)

An element whose text content is only whitespace (spaces, tabs, newlines) is
**not** treated as a content match. It is classified as a selection or
containment node depending on whether it has children. This prevents
formatting whitespace in XML from accidentally creating predicates.

**Tested by:** `p1_14_whitespace_only_text_is_not_content_match`

### Single Quotes in Values (Phase 1 and Phase 2)

When a predicate value contains a single quote (e.g. interface name
`O'Brien`), the algorithm switches to double-quote delimiters:

- Phase 1 output: `[ietf-interfaces:name="O'Brien"]`
- Phase 2 output: `[name="O'Brien"]`

If a value contains both single and double quotes, it falls back to the XPath
`concat()` function.

**Tested by:** `p1_08_value_with_single_quote_uses_double_quotes`,
`p2_20_value_containing_single_quote`

### Slashes in Values (Phase 1)

Interface names like `ge-0/0/0.100` contain `/` characters. These are safely
embedded in the XPath predicate value without escaping — they are inside
quotes and do not affect the path structure.

**Tested by:** `p1_21_content_match_value_with_slash`

### Positional Predicates (Phase 2)

A positional predicate like `[1]` in the subscription XPath does **not** pin
any key value. All list keys are treated as open (`%s` placeholders), because
positional subscripts don't identify a specific instance by key — they refer
to document order, which varies.

**Tested by:** `p2_19_positional_predicate_treated_as_open`

### Module-Prefixed Predicate Keys (Phase 2)

Subscription XPaths may prefix predicate key names with a module name (e.g.
`[ietf-interfaces:name='eth0']`). Phase 2 strips the module prefix, producing
`[name='eth0']` in the template.

**Tested by:** `p2_25_module_prefixed_predicate_key`

### Double-Quoted Predicate Values (Phase 2)

Subscription XPaths may use double quotes (`[name="eth0"]`) instead of single
quotes. Phase 2 normalizes to single-quote output (`[name='eth0']`), unless
the value itself contains a single quote.

**Tested by:** `p2_18_double_quoted_value_normalized`

### Container-Only Subscriptions (Phase 3)

When the subscription targets a YANG container (no list ancestor), there are
no instances to match in the data tree. Phase 3 recognizes this case — if
there are zero extractions and only one branch — and uses the template itself
as the concrete XPath.

**Tested by:** `p3_04_container_produces_fixed_key`

### Multi-Instance Notifications (Phase 3)

A single notification may carry multiple instances (e.g. two interfaces
changed). Phase 3 collects all matching instances, deduplicates them, and
sorts them lexicographically in the `xpaths` array. This guarantees the same
JSON key regardless of the order instances appear in the XML.

**Tested by:** `p3_02_multiple_instances_sorted`,
`p3_08_nested_multiple_inner_instances`

### Cross-Device Key Isolation (Phase 3)

Two devices with the same subscription and the same data tree produce
different Kafka keys because `node_name` is part of the JSON structure.

**Tested by:** `p3_06_same_data_different_nodes_produce_distinct_keys`

### Leaf-List Targets (Phase 2 and Phase 3)

A `leaf-list` target appends `[.='%s']` to the template (or a literal
`[.='value']` if pinned). At runtime, Phase 3 reads the data node's own
canonical value to fill the placeholder.

**Tested by:** `p2_07_leaf_list_target`, `p2_23_concrete_leaf_list_value`,
`p3_09_leaf_list_values`

### `<filter>` Wrapper Auto-Stripping (Phase 1)

If the subtree filter XML has a single root element named `filter` (with no
namespace), it is automatically stripped. The data elements inside it are
treated as top-level.

**Tested by:** `p1_03_filter_wrapper_auto_stripped`

### Duplicate Branch Deduplication (Phase 1)

If the same subtree filter contains duplicate paths (e.g. two identical
`<interface/>` selections), they produce only one XPath branch.

**Tested by:** `p1_09_duplicate_branches_are_deduplicated`

### XML Namespace Inheritance (Phase 1)

Child elements inherit the default `xmlns` namespace from their parent when
they don't declare their own. This is standard XML behavior, but the algorithm
explicitly propagates it during tree parsing.

### Entity Reference Handling (Phase 1)

quick-xml 0.39 does not automatically resolve entity references like `&amp;`.
Instead, it produces `GeneralRef` events that the parser resolves manually for
the five predefined XML entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`,
`&apos;`).

## Building

Requires a C compiler and CMake (for the bundled libyang4 build).

```bash
cargo build --release
```

## Running the Test Suite

```bash
cargo test
```

All 69 tests follow the Arrange-Act-Assert pattern and use external fixture
files from `assets/testdata/` (XML inputs) and `assets/testdata/expected/`
(expected outputs).

### Running Specific Test Groups

```bash
# Only Phase 1 tests (22 tests)
cargo test --test phase1

# Only Phase 2 tests (25 tests)
cargo test --test phase2

# Only Phase 3 tests (9 tests)
cargo test --test phase3

# Only pipeline tests (4 tests)
cargo test --test pipeline

# Only xpath unit tests (9 tests)
cargo test --lib xpath
```

## Comprehensive Test Catalog

### Phase 1 — Subtree Filter Normalization (22 tests)

Each test loads an XML subtree filter from `assets/testdata/`, calls
`normalize_subtree()`, and compares the result to
`assets/testdata/expected/<name>.xpath`.

#### Simple Paths

| Test                                 | Input                                                    | Expected Output                                         |
|--------------------------------------|----------------------------------------------------------|---------------------------------------------------------|
| `p1_01_simple_single_path`           | `<interfaces><interface/></interfaces>`                  | `/ietf-interfaces:interfaces/ietf-interfaces:interface` |
| `p1_03_filter_wrapper_auto_stripped` | `<filter><interfaces><interface/></interfaces></filter>` | Same as above (`<filter>` stripped)                     |
| `p1_04_multiple_top_level_elements`  | Two modules in `<filter>`                                | `.../interface \| .../vlan`                             |

CLI equivalent for `p1_01`:

```bash
yang-push-key phase1 assets/testdata/p1_simple.xml \
    --yang-dir assets/yang -m ietf-interfaces
```

#### Selection Leaves (Union Branches)

| Test                                           | Description                   | Branch count |
|------------------------------------------------|-------------------------------|--------------|
| `p1_05_multiple_leaves_produce_union`          | `<oper-status/>` + `<mtu/>`   | 2 branches   |
| `p1_17_three_leaves_produce_three_branches`    | Three empty siblings          | 3 branches   |
| `p1_20_sibling_selection_leaves`               | `<hostname/>` + `<contact/>`  | 2 branches   |
| `p1_22_mixed_leaf_and_container_at_same_level` | `<hostname/>` + `<dns>` child | 2 branches   |

#### Content Match (Predicates)

| Test                                           | Value pinned                    | Selection node                     |
|------------------------------------------------|---------------------------------|------------------------------------|
| `p1_02_content_match_single_key`               | `name='eth0'`                   | `oper-status`                      |
| `p1_07_content_match_pins_outer_list`          | `name='default'`                | `interface` (inner list)           |
| `p1_10_composite_key_content_match`            | Two keys pinned                 | `metric`                           |
| `p1_12_content_match_with_multiple_selections` | `name='eth0'`                   | `oper-status` + `mtu` (2 branches) |
| `p1_15_content_match_at_both_nesting_levels`   | Outer + inner pinned            | `status`                           |
| `p1_19_dns_server_nested_content_match`        | `name='primary'` in nested list | `address`                          |
| `p1_21_content_match_value_with_slash`         | `name='ge-0/0/0.100'`           | `mtu`                              |

CLI equivalent for `p1_02`:

```bash
yang-push-key phase1 assets/testdata/p1_content.xml \
    --yang-dir assets/yang -m ietf-interfaces
```

#### Special Cases

| Test                                               | What it verifies                        |
|----------------------------------------------------|-----------------------------------------|
| `p1_06_deep_nesting_three_levels`                  | Three-level path works                  |
| `p1_08_value_with_single_quote_uses_double_quotes` | `O'Brien` -> `"O'Brien"`                |
| `p1_09_duplicate_branches_are_deduplicated`        | Identical branches collapsed            |
| `p1_11_container_only`                             | Container target (no list)              |
| `p1_13_container_selecting_specific_leaf`          | `<clock><timezone-utc-offset/></clock>` |
| `p1_14_whitespace_only_text_is_not_content_match`  | Whitespace ignored                      |
| `p1_16_entire_top_level_container`                 | Self-closing `<system/>`                |
| `p1_18_two_modules_in_filter`                      | Cross-module filter                     |

### Phase 2 — Key Template Derivation (25 tests)

Each test calls `derive_templates()` with an XPath and compares the resulting
`key_template` string to `assets/testdata/expected/<name>.template`.

#### Bare XPaths (No Predicates)

| Test                                   | XPath target                    | Template                                         | Extractions |
|----------------------------------------|---------------------------------|--------------------------------------------------|-------------|
| `p2_01_simple_list_single_key`         | `interface`                     | `interface[name='%s']`                           | 1           |
| `p2_02_redundant_prefix_normalized`    | `ietf-interfaces:interface`     | Same (prefix dropped)                            | 1           |
| `p2_03_composite_key_two_leaves`       | `route`                         | `route[destination-prefix='%s'][next-hop='%s']`  | 2           |
| `p2_04_nested_lists`                   | `network-instance/interface`    | `network-instance[name='%s']/interface[id='%s']` | 2           |
| `p2_05_container_only_no_placeholders` | `system/clock`                  | `system/clock`                                   | 0           |
| `p2_06_leaf_inside_list`               | `interface/mtu`                 | `interface[name='%s']/mtu`                       | 1           |
| `p2_07_leaf_list_target`               | `dns/search-domain`             | `search-domain[.='%s']`                          | 1           |
| `p2_08_deep_nested_composite_keys`     | `access-list/access-list-entry` | 3-key template                                   | 3           |
| `p2_10_three_levels_of_nesting`        | `level1/level2/level3`          | 3-key template                                   | 3           |
| `p2_11_leaf_inside_deep_composite`     | `.../access-list-entry/action`  | 3-key + leaf                                     | 3           |

CLI equivalent for `p2_01`:

```bash
yang-push-key phase2 "/ietf-interfaces:interfaces/interface" \
    --yang-dir assets/yang -m ietf-interfaces
```

#### Union XPaths

| Test                                      | Branches | Types                        |
|-------------------------------------------|----------|------------------------------|
| `p2_09_union_two_list_branches`           | 2        | list + list                  |
| `p2_10b_union_three_branches_mixed_types` | 3        | list + container + leaf-list |
| `p2_21_union_one_concrete_one_open`       | 2        | 0 extractions + 1 extraction |

#### Concrete/Predicated XPaths

| Test                                             | What's pinned                      | Extractions         |
|--------------------------------------------------|------------------------------------|---------------------|
| `p2_13_fully_concrete_single_key`                | `[name='eth0']`                    | 0                   |
| `p2_14_fully_concrete_nested`                    | Both list keys                     | 0                   |
| `p2_15_partial_concrete_outer_pinned_inner_open` | `[name='mgmt']`                    | 1 (inner `id`)      |
| `p2_16_fully_concrete_composite_key`             | Both composite keys                | 0                   |
| `p2_17_partial_composite_one_pinned_one_open`    | `[destination-prefix=...]`         | 1 (`next-hop`)      |
| `p2_18_double_quoted_value_normalized`           | `[name="eth0"]` -> `[name='eth0']` | 0                   |
| `p2_19_positional_predicate_treated_as_open`     | `[1]` treated as open              | 2                   |
| `p2_20_value_containing_single_quote`            | `"O'Brien"` preserved              | 0                   |
| `p2_22_concrete_leaf_inside_concrete_list`       | Pinned list + leaf child           | 0                   |
| `p2_23_concrete_leaf_list_value`                 | `[.='example.com']`                | 0                   |
| `p2_24_deep_mixed_outer_pinned_inner_open`       | Composite pinned, inner open       | 1                   |
| `p2_25_module_prefixed_predicate_key`            | `[ietf-interfaces:name=...]`       | 0 (prefix stripped) |

### Phase 3 — Kafka Key Production (9 tests)

Each test derives templates (Phase 2), parses notification XML data, calls
`produce_kafka_key()`, and compares the output JSON to
`assets/testdata/expected/<name>.key`.

| Test                                                    | Scenario                                 | `xpaths` count         |
|---------------------------------------------------------|------------------------------------------|------------------------|
| `p3_01_single_list_instance`                            | One interface                            | 1                      |
| `p3_02_multiple_instances_sorted`                       | Two interfaces (eth1 before eth0 in XML) | 2 (sorted: eth0, eth1) |
| `p3_03_nested_list_instance`                            | Nested network-instance/interface        | 1                      |
| `p3_04_container_produces_fixed_key`                    | Container target (system/clock)          | 1 (template as-is)     |
| `p3_05_composite_key_extraction`                        | Two-key route                            | 1                      |
| `p3_06_same_data_different_nodes_produce_distinct_keys` | Same data, different `node_name`         | Keys differ            |
| `p3_07_leaf_inside_list`                                | Leaf mtu inside interface list           | 1                      |
| `p3_08_nested_multiple_inner_instances`                 | Two inner interfaces                     | 2                      |
| `p3_09_leaf_list_values`                                | Two search-domain values                 | 2                      |

CLI equivalent for `p3_01`:

```bash
yang-push-key phase3 assets/testdata/if_single.xml \
    --xpath "/ietf-interfaces:interfaces/interface" \
    --node-name router-nyc-01 --sub-id 1042 \
    --yang-dir assets/yang -m ietf-interfaces
```

CLI equivalent for `p3_02` (multiple instances):

```bash
yang-push-key phase3 assets/testdata/if_multi.xml \
    --xpath "/ietf-interfaces:interfaces/interface" \
    --node-name router-nyc-01 --sub-id 1042 \
    --yang-dir assets/yang -m ietf-interfaces
```

### Pipeline — End-to-End (4 tests)

| Test                                                | Phases  | Description                           |
|-----------------------------------------------------|---------|---------------------------------------|
| `pipeline_p1_to_p2_pinned_key_and_container`        | 1->2    | Content match + container, 2 branches |
| `pipeline_p1_to_p2_outer_pinned_inner_open`         | 1->2    | Outer list pinned, inner open         |
| `pipeline_p1_to_p2_to_p3_full_roundtrip`            | 1->2->3 | Simple filter -> JSON key             |
| `pipeline_p1_to_p2_to_p3_nested_with_content_match` | 1->2->3 | Nested content match -> JSON key      |

CLI equivalent for full pipeline:

```bash
yang-push-key pipeline assets/testdata/if_single.xml \
    --subtree assets/testdata/p1_simple.xml \
    --node-name router-nyc-01 --sub-id 1042 \
    --yang-dir assets/yang -m ietf-interfaces
```

### XPath Unit Tests (9 tests)

Located in [`src/xpath.rs`](src/xpath.rs). These test the low-level parsing
utilities in isolation:

| Test                             | Function             | What it verifies                |
|----------------------------------|----------------------|---------------------------------|
| `split_simple_union`             | `split_union`        | Basic `\|` splitting            |
| `split_preserves_predicate_pipe` | `split_union`        | `\|` inside `[...]` not split   |
| `split_preserves_quoted_pipe`    | `split_union`        | `\|` inside `"..."` not split   |
| `parse_single_predicate`         | `parse_predicates`   | `[name='eth0']` parsed          |
| `parse_positional`               | `parse_predicates`   | `[1]` flagged as positional     |
| `parse_strips_prefix`            | `parse_predicates`   | `[mod:name=...]` prefix removed |
| `escape_simple_value`            | `escape_xpath_value` | `eth0` -> `'eth0'`              |
| `escape_value_with_single_quote` | `escape_xpath_value` | `O'Brien` -> `"O'Brien"`        |
| `strip_predicates_from_path`     | `strip_predicates`   | `[k='v']` removed               |

## CLI Usage Reference

All subcommands require `--yang-dir` and either at least one `-m` module flag
or a `--yang-library` file (or both).

### Schema Loading

There are two ways to tell the CLI which YANG modules to load:

#### Option A: Individual modules with `-m`

List each module explicitly. Use `NAME:FEATURE1,FEATURE2` syntax to enable
YANG features:

```bash
yang-push-key phase2 "/ietf-interfaces:interfaces/interface" \
    --yang-dir assets/yang \
    -m ietf-interfaces \
    -m ietf-ip:ipv4-non-contiguous-netmasks,ipv6-privacy-autoconf
```

#### Option B: YANG Library file (`--yang-library`)

Provide an [RFC 8525](https://datatracker.ietf.org/doc/html/rfc8525) YANG
Library file (XML or JSON) that describes all implemented modules, their
revisions, and enabled features. This is especially useful when many modules
are needed and a network device already exports its YANG Library.

An example YANG Library file is included at
[`assets/yang/yang-library-interfaces.xml`](assets/yang/yang-library-interfaces.xml).
It declares `ietf-interfaces` (with `arbitrary-names`, `pre-provisioning`,
`if-mib` features) and `ietf-ip` (with `ipv4-non-contiguous-netmasks`,
`ipv6-privacy-autoconf` features):

```bash
yang-push-key phase2 "/ietf-interfaces:interfaces/interface" \
    --yang-dir assets/yang \
    --yang-library assets/yang/yang-library-interfaces.xml
```

The data format is auto-detected from the file extension (`.xml` → XML,
`.json` → JSON). Override with `--yang-library-format`:

```bash
yang-push-key phase2 "/ietf-interfaces:interfaces/interface" \
    --yang-dir assets/yang \
    --yang-library yang-lib.dat --yang-library-format xml
```

#### Combining both modes

The YANG Library is loaded first, then any `-m` modules are loaded on top.
This is handy when a base library exists but you need an extra module:

```bash
yang-push-key phase2 "/ietf-system:system/dns-resolver/search" \
    --yang-dir assets/yang \
    --yang-library assets/yang/yang-library-interfaces.xml \
    -m ietf-system
```

### Phase 1 — Subtree Filter to XPath

```bash
# With individual modules:
yang-push-key phase1 <SUBTREE_FILE> --yang-dir <DIR> -m <MODULE> [-m <MODULE>...]

# With YANG Library:
yang-push-key phase1 <SUBTREE_FILE> --yang-dir <DIR> --yang-library <FILE>
```

Example using the included YANG Library:

```bash
yang-push-key phase1 assets/testdata/p1_simple.xml \
    --yang-dir assets/yang \
    --yang-library assets/yang/yang-library-interfaces.xml
# Output: /ietf-interfaces:interfaces/ietf-interfaces:interface
```

### Phase 2 — XPath to Key Template (JSON)

```bash
# With individual modules:
yang-push-key phase2 <XPATH> --yang-dir <DIR> -m <MODULE> [-m <MODULE>...]

# With YANG Library:
yang-push-key phase2 <XPATH> --yang-dir <DIR> --yang-library <FILE>
```

Example — derive a key template for the IP address list (augmented by ietf-ip
onto ietf-interfaces):

```bash
yang-push-key phase2 \
    "/ietf-interfaces:interfaces/interface/ietf-ip:ipv4/address" \
    --yang-dir assets/yang \
    --yang-library assets/yang/yang-library-interfaces.xml
```

Output (abbreviated):

```json
{
  "subscription_xpath": "/ietf-interfaces:interfaces/interface/ietf-ip:ipv4/address",
  "branches": [
    {
      "branch_index": 0,
      "branch_xpath": "/ietf-interfaces:interfaces/interface/ietf-ip:ipv4/address",
      "key_template": "/ietf-interfaces:interfaces/interface[name='%s']/ietf-ip:ipv4/address[ip='%s']",
      "target_type": "list",
      "extractions": [
        { "placeholder_index": 0, "key_leaf": "name",  "list_module": "ietf-interfaces", "list_name": "interface" },
        { "placeholder_index": 1, "key_leaf": "ip",    "list_module": "ietf-ip",         "list_name": "address"   }
      ]
    }
  ]
}
```

### Phase 3 — Notification Data to Kafka Key (JSON)

```bash
# With individual modules:
yang-push-key phase3 <DATA_FILE> --xpath <XPATH> --node-name <NAME> --sub-id <ID> \
    --yang-dir <DIR> -m <MODULE> [-m <MODULE>...]

# With YANG Library:
yang-push-key phase3 <DATA_FILE> --xpath <XPATH> --node-name <NAME> --sub-id <ID> \
    --yang-dir <DIR> --yang-library <FILE>
```

### Pipeline — Full End-to-End (JSON)

```bash
# With individual modules:
yang-push-key pipeline <DATA_FILE> --subtree <SUBTREE_FILE> \
    --node-name <NAME> --sub-id <ID> \
    --yang-dir <DIR> -m <MODULE> [-m <MODULE>...]

# With YANG Library:
yang-push-key pipeline <DATA_FILE> --subtree <SUBTREE_FILE> \
    --node-name <NAME> --sub-id <ID> \
    --yang-dir <DIR> --yang-library <FILE>
```

### Stdin Support

Use `-` as the file path to read from stdin:

```bash
cat notification.xml | yang-push-key phase3 - \
    --xpath "/ietf-interfaces:interfaces/interface" \
    --node-name router-01 --sub-id 1042 \
    --yang-dir ./yang -m ietf-interfaces
```

### Module Features

Load modules with specific YANG features enabled using colon-separated syntax:

```bash
-m ietf-ip:ipv4-non-contiguous-netmasks,ipv6-privacy-autoconf
```

## Project Structure

```
yang-push-key-rs/
+-- Cargo.toml
+-- README.md
+-- src/
|   +-- lib.rs          # Crate root, module declarations, re-exports
|   +-- types.rs        # Shared data structures (KafkaKey, DerivationResult, etc.)
|   +-- xpath.rs        # XPath parsing utilities + 9 unit tests
|   +-- phase1.rs       # Phase 1: subtree filter normalization (quick-xml 0.39)
|   +-- phase2.rs       # Phase 2: key template derivation
|   +-- phase3.rs       # Phase 3: runtime Kafka key production (JSON output)
|   +-- main.rs         # CLI (clap)
+-- tests/
|   +-- common.rs       # Test helpers (create_ctx, parse_data)
|   +-- phase1.rs       # 22 Phase 1 integration tests
|   +-- phase2.rs       # 25 Phase 2 integration tests
|   +-- phase3.rs       # 9 Phase 3 integration tests
|   +-- pipeline.rs     # 4 end-to-end pipeline tests
+-- assets/
    +-- yang/           # 7 YANG schema files
    |   +-- yang-library-interfaces.xml  # Example YANG Library (RFC 8525)
    +-- testdata/       # 31 XML input fixtures
        +-- expected/   # 43 expected output files (.xpath, .template, .key)
```

## YANG Schemas Used in Tests

| Module            | Namespace                                     | Key Structure                                                   |
|-------------------|-----------------------------------------------|-----------------------------------------------------------------|
| `ietf-interfaces` | `urn:ietf:params:xml:ns:yang:ietf-interfaces` | `interface[name]`                                               |
| `example-routes`  | `urn:example:routes`                          | `route[destination-prefix, next-hop]` (composite)               |
| `example-network` | `urn:example:network`                         | `network-instance[name]/interface[id]` (nested)                 |
| `ietf-system`     | `urn:ietf:params:xml:ns:yang:ietf-system`     | Container + `leaf-list` + `list server[name]`                   |
| `example-vlans`   | `urn:example:vlans`                           | `vlan[vlan-id]` (uint16 key)                                    |
| `example-acl`     | `urn:example:acl`                             | `access-list[name, type]/entry[sequence-id]` (nested composite) |
| `example-deep`    | `urn:example:deep`                            | `level1/level2/level3` (3-level nesting)                        |

## Library Usage

```rust
use yang_push_key::{normalize_subtree, derive_templates, produce_kafka_key};

// Phase 1 (optional — only needed for subtree filter subscriptions)
let xpath = normalize_subtree( & ctx, subtree_xml) ?;

// Phase 2 (once per subscription, at subscription creation time)
let derivation = derive_templates( & ctx, & xpath) ?;

// Phase 3 (per notification, at runtime)
let result = produce_kafka_key( & derivation, & data_tree, "router-01", "1042") ?;

// result.kafka_key  — compact JSON string for Kafka ProducerRecord key
// result.key        — structured KafkaKey for programmatic access
println!("{}", result.kafka_key);
println!("{}", result.key.node_name);         // "router-01"
println!("{}", result.key.subscription_id);   // "1042"
for xpath in & result.key.xpaths {
println!("  {}", xpath);
}
```

## License

Apache 2.0, 
Copyright: Ahmed Elhassany

