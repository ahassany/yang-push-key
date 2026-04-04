# yang-push-key

Derive unique message broker **message keys** and **topic names** from
[YANG Push](https://www.rfc-editor.org/rfc/rfc8641) on-change notifications,
as defined in
[draft-ietf-nmop-yang-message-broker-message-key-03](assets/docs/draft-ietf-nmop-yang-message-broker-message-key-03.xml).

YANG Push allows network devices to stream configuration and state changes as
notifications. When these notifications are published to a message broker
compact topic, each message needs a **deterministic key** so that log
compaction retains only the most recent value for each logical piece of state.
This tool solves the problem with a three-phase algorithm that normalizes
subscription variants, resolves them against the YANG schema, and produces a
compact, line-delimited key at notification delivery time.

Additionally, the tool derives **topic names** from subscription schema paths,
producing human-readable, regex-friendly, stable names suitable for Apache
Kafka and similar message brokers.

```
  Subtree Filter ──► Phase 1 ──► Normalized XPath(s)
                                        │
  Subscription XPath ──────────────────►│
  YANG Schema ─────────────────────────►│
                                        ▼
                                  Phase 2 ──► Key Template + Extraction Specs
                                        │         │
  Notification Data ───────────────────►│         ▼
  Node Name + Subscription ID ─────────►│   Topic Name Derivation
                                        ▼
                                  Phase 3 ──► Message Key (line-delimited)
```

---

## Table of Contents

- [CLI Usage](#cli-usage)
  - [Schema Loading](#schema-loading)
  - [phase1 — Subtree Filter to XPath](#phase1--subtree-filter-to-xpath)
  - [phase2 — XPath to Key Template](#phase2--xpath-to-key-template)
  - [phase3 — Notification Data to Message Key](#phase3--notification-data-to-message-key)
  - [pipeline — Full End-to-End](#pipeline--full-end-to-end)
  - [topic — Topic Name Derivation](#topic--topic-name-derivation)
- [Library Usage (Rust API)](#library-usage-rust-api)
- [Algorithms](#algorithms)
  - [Message Key Algorithm](#message-key-algorithm)
  - [Topic Name Algorithm](#topic-name-algorithm)
- [Development](#development)
  - [Building](#building)
  - [Running Tests](#running-tests)
  - [Project Structure](#project-structure)
  - [YANG Schemas Used in Tests](#yang-schemas-used-in-tests)
  - [Exploring YANG Schemas with yanglint](#exploring-yang-schemas-with-yanglint)
- [License](#license)

---

## CLI Usage

The binary is called `yang-push-key`. Every subcommand requires `--yang-dir`
pointing at the directory containing `.yang` files, plus at least one of `-m`
(individual module) or `--yang-library` (RFC 8525 file).

### Schema Loading

There are two ways to specify which YANG modules to load — they can be
combined.

#### Individual modules (`-m`)

List each module explicitly. Use `NAME:FEATURE1,FEATURE2` to enable YANG
features:

```bash
yang-push-key phase2 "/ietf-interfaces:interfaces/interface" \
    --yang-dir assets/yang \
    -m ietf-interfaces \
    -m ietf-ip:ipv4-non-contiguous-netmasks,ipv6-privacy-autoconf
```

#### YANG Library (`--yang-library`)

Provide an [RFC 8525](https://datatracker.ietf.org/doc/html/rfc8525) YANG
Library file (XML or JSON) that lists all implemented modules, revisions, and
features. This is the easiest approach when a network device already exports
its YANG Library.

An example is included at
[`assets/yang/yang-library-interfaces.xml`](assets/yang/yang-library-interfaces.xml).

```bash
yang-push-key phase2 "/ietf-interfaces:interfaces/interface" \
    --yang-dir assets/yang \
    --yang-library assets/yang/yang-library-interfaces.xml
```

Format is auto-detected from the file extension (`.xml` / `.json`). Override
with `--yang-library-format xml|json`.

#### Combining both

The YANG Library is loaded first, then `-m` modules on top — handy when a
base library exists but you need an extra module:

```bash
yang-push-key phase2 "/ietf-system:system/dns-resolver/search" \
    --yang-dir assets/yang \
    --yang-library assets/yang/yang-library-interfaces.xml \
    -m ietf-system
```

#### Stdin support

Use `-` as the file path to read from stdin:

```bash
cat notification.xml | yang-push-key phase3 - \
    --xpath "/ietf-interfaces:interfaces/interface" \
    --node-name router-01 --sub-id 1042 \
    --yang-dir ./yang -m ietf-interfaces
```

---

### phase1 — Subtree Filter to XPath

Converts an RFC 6241 subtree filter (XML) into one or more XPath expressions.

```bash
yang-push-key phase1 <SUBTREE_FILE> --yang-dir <DIR> -m <MODULE>
```

Example:

```bash
yang-push-key phase1 assets/testdata/p1_simple.xml \
    --yang-dir assets/yang -m ietf-interfaces
# Output: /ietf-interfaces:interfaces/ietf-interfaces:interface
```

### phase2 — XPath to Key Template

Derives key template(s) and extraction specs from a subscription XPath.
Prints JSON output.

```bash
yang-push-key phase2 <XPATH> --yang-dir <DIR> -m <MODULE>
```

Example:

```bash
yang-push-key phase2 "/ietf-interfaces:interfaces/interface" \
    --yang-dir assets/yang -m ietf-interfaces
```

Output (abbreviated):

```json
{
  "subscription_xpath": "/ietf-interfaces:interfaces/interface",
  "branches": [
    {
      "branch_index": 0,
      "key_template": "/ietf-interfaces:interfaces/interface[name='%s']",
      "target_type": "list",
      "extractions": [
        {
          "extraction_xpath": "/ietf-interfaces:interfaces/interface/name",
          "key_leaf": "name",
          "list_module": "ietf-interfaces",
          "list_name": "interface"
        }
      ]
    }
  ]
}
```

### phase3 — Notification Data to Message Key

Produces a message key from a subscription XPath and notification data.
Internally runs Phase 2 first.

```bash
yang-push-key phase3 <DATA_FILE> --xpath <XPATH> \
    --node-name <NAME> --sub-id <ID> \
    --yang-dir <DIR> -m <MODULE>
```

Example (single interface):

```bash
yang-push-key phase3 assets/testdata/if_single.xml \
    --xpath "/ietf-interfaces:interfaces/interface" \
    --node-name router-nyc-01 --sub-id 1042 \
    --yang-dir assets/yang -m ietf-interfaces
```

Output:

```
router-nyc-01
1042
/ietf-interfaces:interfaces/interface[name='eth0']
```

Example (multiple instances — keys sorted and joined):

```bash
yang-push-key phase3 assets/testdata/if_multi.xml \
    --xpath "/ietf-interfaces:interfaces/interface" \
    --node-name router-nyc-01 --sub-id 1042 \
    --yang-dir assets/yang -m ietf-interfaces
```

Output:

```
router-nyc-01
1042
/ietf-interfaces:interfaces/interface[name='eth0'] | /ietf-interfaces:interfaces/interface[name='eth1']
```

### pipeline — Full End-to-End

Runs Phase 1 → 2 → 3 in one shot. Takes a subtree filter and notification
data, produces the final message key.

```bash
yang-push-key pipeline <DATA_FILE> --subtree <SUBTREE_FILE> \
    --node-name <NAME> --sub-id <ID> \
    --yang-dir <DIR> -m <MODULE>
```

Example:

```bash
yang-push-key pipeline assets/testdata/if_single.xml \
    --subtree assets/testdata/p1_simple.xml \
    --node-name router-nyc-01 --sub-id 1042 \
    --yang-dir assets/yang -m ietf-interfaces
```

### topic — Topic Name Derivation

Derives message broker topic names from a subscription XPath. Runs Phase 2
internally, strips predicates, replaces YANG module names with their short
prefixes, and prints one topic name per line (one per union branch).

```bash
yang-push-key topic <XPATH> --yang-dir <DIR> -m <MODULE> [--prefix <PREFIX>] [--max-length <N>]
```

Examples:

```bash
# Simple topic name
yang-push-key topic "/ietf-interfaces:interfaces/interface" \
    --yang-dir assets/yang -m ietf-interfaces
# Output: if-interfaces-interface

# With organization prefix
yang-push-key topic "/ietf-interfaces:interfaces/interface" \
    --prefix netops --yang-dir assets/yang -m ietf-interfaces
# Output: netops-if-interfaces-interface

# Leaf target
yang-push-key topic "/ietf-interfaces:interfaces/interface/oper-status" \
    --yang-dir assets/yang -m ietf-interfaces
# Output: if-interfaces-interface-oper-status

# System module
yang-push-key topic "/ietf-system:system/clock" \
    --yang-dir assets/yang -m ietf-system
# Output: sys-system-clock

# With YANG Library
yang-push-key topic "/ietf-interfaces:interfaces/interface/ietf-ip:ipv4/address" \
    --yang-dir assets/yang \
    --yang-library assets/yang/yang-library-interfaces.xml
# Output: if-interfaces-interface-ip-ipv4-address
```

| Option | Description |
|--------|-------------|
| `--prefix <PREFIX>` | Organization/team prefix prepended to every topic name (e.g. `netops`, `platform-team`) |
| `--max-length <N>` | Maximum topic name length (default: 255). Names exceeding this are truncated with an FNV-1a hash suffix |

---

## Library Usage (Rust API)

Add the dependency:

```toml
[dependencies]
yang-push-key = { path = "." }
```

### Message Key Derivation

```rust
use yang_push_key::{normalize_subtree, derive_templates, produce_message_key};

// Phase 1 (optional — only needed for subtree filter subscriptions)
let xpath = normalize_subtree(&ctx, subtree_xml)?;

// Phase 2 (once per subscription, at subscription creation time)
let derivation = derive_templates(&ctx, &xpath)?;

// Phase 3 (per notification, at runtime)
let result = produce_message_key(&derivation, &data_tree, "router-01", "1042")?;

// result.message_key  — line-delimited string for message broker record key
// result.key          — structured MessageKey for programmatic access
println!("{}", result.message_key);
println!("{}", result.key.node_name);         // "router-01"
println!("{}", result.key.subscription_id);   // "1042"
for xpath in &result.key.xpaths {
    println!("  {}", xpath);
}
```

### Topic Name Derivation

```rust
use yang_push_key::{derive_templates, derive_topic_names, TopicConfig};

let derivation = derive_templates(&ctx, "/ietf-interfaces:interfaces/interface")?;

// Default config (no prefix, max 255 chars)
let config = TopicConfig::new();
let topics = derive_topic_names(&ctx, &derivation, &config)?;
println!("{}", topics.topic_names[0]); // "if-interfaces-interface"

// With organization prefix and custom max length
let config = TopicConfig::new()
    .with_prefix("netops")
    .with_max_length(200);
let topics = derive_topic_names(&ctx, &derivation, &config)?;
println!("{}", topics.topic_names[0]); // "netops-if-interfaces-interface"
```

---

## Algorithms

### Message Key Algorithm

The algorithm has three phases. Phase 1 is only needed for subtree filter
subscriptions; XPath subscriptions skip to Phase 2. Phase 2 runs once at
subscription creation time. Phase 3 runs for every notification.

#### Message Key Format

The output is a line-delimited UTF-8 string with exactly three fields
separated by newline (LF, U+000A):

```
router-nyc-01
1042
/ietf-interfaces:interfaces/interface[name='eth0']
```

| Line | Description |
|------|-------------|
| 1 | `node_name` — Managed node identifier (hostname, FQDN, etc.) |
| 2 | `subscription_id` — YANG Push subscription ID |
| 3 | Concrete XPaths, sorted lexicographically, joined by ` \| ` |

When multiple instances change in a single notification, all concrete XPaths
appear on the third line, sorted and deduplicated:

```
router-nyc-01
1042
/ietf-interfaces:interfaces/interface[name='eth0'] | /ietf-interfaces:interfaces/interface[name='eth1']
```

No trailing newline after the XPath line.

#### Phase 1: Subtree Filter Normalization

**Module:** [`src/phase1.rs`](src/phase1.rs) — `normalize_subtree()`

Converts an RFC 6241 subtree filter (XML) into equivalent XPath expressions
joined by ` | `.

Each child element is classified per RFC 6241 Section 6.2:

| Classification | Has text? | Has children? | Treatment |
|----------------|-----------|---------------|-----------|
| **Content match** | yes | no | Becomes `[qname='value']` predicate |
| **Selection** | no | no | Terminal node — ends a branch |
| **Containment** | no / whitespace | yes | Intermediate — recurse into children |

Phase 1 emits **full module-name prefixes** on every segment
(`/ietf-interfaces:interfaces/ietf-interfaces:interface`). Phase 2 normalizes
these to minimal-prefix style.

#### Phase 2: Key Template Derivation

**Module:** [`src/phase2.rs`](src/phase2.rs) — `derive_templates()`

Resolves each union branch to its target schema node, walks the ancestor chain
from root to target, and builds a key template.

**Template format** — minimal-prefix style (module name only on first segment
or when it changes). List key predicates use `%s` for values extracted at
runtime, literal values for statically pinned keys:

```
/ietf-interfaces:interfaces/interface[name='%s']
/example-routes:routes/route[destination-prefix='10.0.0.0/8'][next-hop='%s']
```

**Extraction format** — each `%s` placeholder has a corresponding extraction
XPath: a full absolute path from root to the key leaf:

| Template | Extraction |
|----------|-----------|
| `/ietf-interfaces:interfaces/interface[name='%s']` | `/ietf-interfaces:interfaces/interface/name` |
| `.../interface[name='eth0']/ietf-ip:ipv4/address[ip='%s']` | `.../interface[name='eth0']/ietf-ip:ipv4/address/ip` |

**Predicate normalization:**

| Input form | Template output |
|------------|-----------------|
| No predicate (bare path) | `[key='%s']` + extraction |
| `[key='value']` | `[key='value']` (literal) |
| `[mod:key='value']` | `[key='value']` (prefix stripped) |
| `[key="value"]` | `[key='value']` (normalized to single-quote) |
| `[N]` (positional) | `[key='%s']` (treated as open) |

#### Phase 3: Runtime Message Key Production

**Module:** [`src/phase3.rs`](src/phase3.rs) — `produce_message_key()`

Walks the notification data tree, matches instances to branch templates,
extracts key leaf values (using an optimized ancestor tree walk — O(d) where d
is the tree depth), fills placeholders, deduplicates and sorts the concrete
XPaths, and composes the line-delimited message key.

#### Special Cases

<details>
<summary>Click to expand special case handling details</summary>

**Content match vs. whitespace (Phase 1):** Elements with whitespace-only text
are not treated as content matches — prevents formatting whitespace from
creating predicates.

**Single quotes in values:** When a value contains `'`, double-quote delimiters
are used (`[name="O'Brien"]`). If both quote types appear, XPath `concat()` is
used.

**Slashes in values:** Interface names like `ge-0/0/0.100` are safely embedded
in predicate values without escaping.

**Positional predicates:** `[1]` does not pin keys — all list keys become open
`%s` placeholders.

**Container-only subscriptions:** When the target is a container (no list
ancestor), Phase 3 uses the template itself as the concrete XPath.

**Multi-instance notifications:** Multiple changed instances are collected,
deduplicated, and sorted on the third key line.

**Cross-device isolation:** Two devices with the same subscription produce
different keys because `node_name` is line 1.

**Leaf-list targets:** `[.='%s']` is appended (or literal `[.='value']` if
pinned). Phase 3 reads the node's own value.

**`<filter>` wrapper:** Automatically stripped if the root element is `<filter>`
with no namespace.

**Duplicate branches:** Identical XPath branches are deduplicated.

**XML namespace inheritance:** Child elements inherit `xmlns` from parents.

</details>

---

### Topic Name Algorithm

Topic names are derived from Phase 2 key templates through a deterministic,
schema-path-only transformation. They are **stable under schema evolution** —
augmenting the schema adds new topic names but never changes existing ones.

For the full rationale (including why information-theoretic optimizations are
unsafe), see
[`assets/docs/topic-name-algorithm.md`](assets/docs/topic-name-algorithm.md).

#### Steps

1. **Strip predicates** — remove all `[...]` from the Phase 2 template to get
   the pure schema DATA path.
2. **Replace module names with YANG prefixes** — look up each module's short
   `prefix` statement (e.g. `ietf-interfaces` → `if`).
3. **Flatten** — remove leading `/`, replace `:` and `/` with `-`.
4. **Prepend organization prefix** (optional) — `if-interfaces-interface` →
   `netops-if-interfaces-interface`.
5. **Truncate if needed** — if the name exceeds the max length (default 255),
   truncate at a `-` boundary and append an 8-character FNV-1a hash suffix.

#### Examples

| Subscription target | Topic name |
|---|---|
| `/ietf-interfaces:interfaces/interface` | `if-interfaces-interface` |
| `/ietf-interfaces:interfaces/interface/oper-status` | `if-interfaces-interface-oper-status` |
| `/ietf-system:system/clock` | `sys-system-clock` |
| `/ietf-system:system/dns-resolver/server` | `sys-system-dns-resolver-server` |
| `/example-network:network-instances/network-instance/interface` | `ni-network-instances-network-instance-interface` |
| `/example-routes:routes/route` | `rt-routes-route` |

#### Properties

- **Deterministic** — pure function of the schema path and YANG prefixes.
- **Collision-free** — the mapping is injective (reversible).
- **Stable** — depends only on the path, not on sibling nodes.
- **Regex-friendly** — `^if-interfaces-interface-.*` matches all interface
  fields; `^if-.*` matches everything from `ietf-interfaces`.
- **Readable** — YANG prefix identifies the module, node names are unabbreviated.

---

## Development

### Building

Requires a C compiler and CMake (for the bundled libyang4 build).

```bash
cargo build --release
```

### Running Tests

```bash
cargo test
```

All 116 tests follow the Arrange-Act-Assert pattern and use fixture files from
`assets/testdata/` (inputs) and `assets/testdata/expected/` (expected outputs).

```bash
# By phase
cargo test --test phase1      # 22 Phase 1 tests
cargo test --test phase2      # 25 Phase 2 tests
cargo test --test phase3      #  9 Phase 3 tests
cargo test --test pipeline    #  4 end-to-end pipeline tests
cargo test --test topic       # 26 topic name tests

# Unit tests (xpath, types, topic internals)
cargo test --lib
```

### Project Structure

```
yang-push-key-rs/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs          # Crate root, module declarations, re-exports
│   ├── types.rs        # Shared types (MessageKey, DerivationResult, ExtractionSpec, etc.)
│   ├── xpath.rs        # XPath parsing utilities (split_union, parse_predicates, etc.)
│   ├── phase1.rs       # Phase 1: subtree filter normalization (quick-xml)
│   ├── phase2.rs       # Phase 2: key template derivation
│   ├── phase3.rs       # Phase 3: runtime message key production (line-delimited)
│   ├── topic.rs        # Topic name derivation (prefix lookup, flatten, truncate)
│   └── main.rs         # CLI (clap) — phase1, phase2, phase3, pipeline, topic
├── tests/
│   ├── common.rs       # Test helpers (create_ctx, parse_data)
│   ├── phase1.rs       # 22 Phase 1 integration tests
│   ├── phase2.rs       # 25 Phase 2 integration tests
│   ├── phase3.rs       #  9 Phase 3 integration tests
│   ├── pipeline.rs     #  4 end-to-end pipeline tests
│   └── topic.rs        # 26 topic name tests
└── assets/
    ├── docs/           # IETF draft XML, topic-name-algorithm.md
    ├── yang/           # YANG schema files + example YANG Library
    │   └── yang-library-interfaces.xml   # Example RFC 8525 YANG Library
    └── testdata/       # XML input fixtures
        └── expected/   # Expected output files (.xpath, .template, .key)
```

### YANG Schemas Used in Tests

| Module | Key Structure |
|--------|---------------|
| `ietf-interfaces` | `interface[name]` |
| `ietf-ip` | Augments `ietf-interfaces` with `ipv4/address[ip]`, `ipv6/address[ip]` |
| `ietf-system` | Container + `leaf-list` + `list server[name]` |
| `example-routes` | `route[destination-prefix, next-hop]` (composite key) |
| `example-network` | `network-instance[name]/interface[id]` (nested lists) |
| `example-vlans` | `vlan[vlan-id]` (uint16 key) |
| `example-acl` | `access-list[name, type]/entry[sequence-id]` (nested composite) |
| `example-deep` | `level1/level2/level3` (3-level nesting) |

### Exploring YANG Schemas with yanglint

<details>
<summary>Click to expand yanglint tutorial</summary>

[`yanglint`](https://netopeer.liberouter.org/doc/libyang/master/html/howto_yanglint.html)
is the CLI tool shipped with libyang. You can use it to inspect YANG modules,
discover list keys, and understand the XPath structure this tool operates on.

#### Print the full schema tree

```bash
yanglint -p assets/yang -f tree assets/yang/ietf-interfaces@2018-02-20.yang
```

```
module: ietf-interfaces
  +--rw interfaces
     +--rw interface* [name]
        +--rw name                        string
        +--rw description?                string
        +--rw type                        identityref
        +--rw enabled?                    boolean
        +--ro oper-status                 enumeration
        +--ro statistics
           +--ro discontinuity-time    yang:date-and-time
           +--ro in-octets?            yang:counter64
           ...
```

The `[name]` annotation tells you that `name` is the list key — the value
Phase 2 turns into `[name='%s']` and Phase 3 fills from notification data.

#### Inspect a single node

```bash
yanglint -p assets/yang \
    -P "/ietf-interfaces:interfaces/interface" -q -f tree \
    assets/yang/ietf-interfaces@2018-02-20.yang
```

```
module: ietf-interfaces
  +--rw interfaces
     +--rw interface* [name]
```

#### Show detailed node information

```bash
yanglint -p assets/yang \
    -P "/ietf-interfaces:interfaces/interface" -q -f info \
    assets/yang/ietf-interfaces@2018-02-20.yang
```

```
list interface {
  key "name";
  config true;
  min-elements 0;
  max-elements 4294967295;
  ordered-by system;
  status current;
  ...
}
```

#### Composite keys

```bash
yanglint -p assets/yang -f tree assets/yang/example-routes.yang
```

```
module: example-routes
  +--rw routes
     +--rw route* [destination-prefix next-hop]
        +--rw destination-prefix    string
        +--rw next-hop              string
        +--rw metric?               uint32
```

Phase 2 produces a template with two placeholders for this list:
`route[destination-prefix='%s'][next-hop='%s']`.

#### Nested lists

Lists can be nested. In `example-network`, each `network-instance` contains
an inner `interface` list with its own key:

```bash
yanglint -p assets/yang -f tree assets/yang/example-network.yang
```

```
module: example-network
  +--rw network-instances
     +--rw network-instance* [name]
        +--rw name    string
        +--rw interface* [id]
           +--rw id        string
           +--rw status?   string
```

Phase 2 produces: `network-instance[name='%s']/interface[id='%s']` — two
extractions, one for each list level.

#### Containers, leaf-lists, and mixed structures

The `ietf-system` module illustrates containers (no key), leaf-lists, and
nested lists all in one tree:

```bash
yanglint -p assets/yang -f tree assets/yang/ietf-system@2014-08-06.yang
```

```
module: ietf-system
  +--rw system
     +--rw contact?          string
     +--rw hostname?         inet:domain-name
     +--rw clock
     |  +--rw (timezone)?
     |     ...
     +--rw dns-resolver
        +--rw search*    inet:domain-name    ← leaf-list (no key, value is the key)
        +--rw server* [name]                 ← list keyed by name
        |  +--rw name           string
        |  ...
```

- `system/clock` is a **container** — Phase 2 emits it as-is with no
  placeholders.
- `dns-resolver/search` is a **leaf-list** — Phase 2 appends `[.='%s']`.
- `dns-resolver/server` is a **list** — Phase 2 appends `[name='%s']`.

#### Validate data against schema

```bash
yanglint -n -p assets/yang -t data -f json \
    assets/yang/ietf-interfaces@2018-02-20.yang \
    assets/testdata/if_single.xml
```

```json
{
  "ietf-interfaces:interfaces": {
    "interface": [
      {
        "name": "eth0",
        "oper-status": "up"
      }
    ]
  }
}
```

The JSON output shows the module-prefixed top-level node
(`ietf-interfaces:interfaces`) — the same prefix style that Phase 2 uses in
key templates. The `-n` flag relaxes validation so partial data (like a
notification snippet missing mandatory leaves) is accepted.

#### Extract key values from notification data

Phase 2 extraction XPaths can be passed directly to yanglint's `-E XPATH`
flag to verify key extraction:

```bash
# Extract interface names from notification data
yanglint -n -p assets/yang -t data \
    -E "/ietf-interfaces:interfaces/interface/name" \
    assets/yang/ietf-interfaces@2018-02-20.yang \
    assets/testdata/if_multi.xml

# Nested list — outer key (network-instance name)
yanglint -n -p assets/yang -t data \
    -E "/example-network:network-instances/network-instance/name" \
    assets/yang/example-network.yang \
    assets/testdata/ni_single.xml

# Nested list — inner key (interface id)
yanglint -n -p assets/yang -t data \
    -E "/example-network:network-instances/network-instance/interface/id" \
    assets/yang/example-network.yang \
    assets/testdata/ni_single.xml
```

</details>

---

## License

Apache 2.0,
Copyright: Ahmed Elhassany

