# Kafka Topic Name Derivation from YANG Push Subscriptions

## Problem

When publishing YANG Push notifications to Apache Kafka, each subscription
needs a deterministic topic name that groups related messages. The topic name
must satisfy competing constraints:

1. **Deterministic.** The same subscription, regardless of syntactic form (XPath
   vs. subtree filter, single-quoted vs. double-quoted predicates, redundant
   module prefixes), must always map to the same topic.

2. **Unique.** Two subscriptions targeting different schema nodes must never
   share a topic.

3. **Human-readable.** An operator looking at a topic listing should be able to
   identify what data the topic carries.

4. **Regex-friendly.** A consumer should be able to subscribe to related topics
   using a simple pattern — for example, all fields of a particular list.

5. **Stable under schema evolution.** Augmenting the YANG schema with new nodes
   must not change existing topic names.

6. **Within Kafka limits.** Topic names may contain `[a-zA-Z0-9._-]` and must
   not exceed 249 characters.

## The Key Insight

The existing Phase 2 key template derivation already normalizes every
subscription form into a canonical schema path. Stripping the predicates
from the Phase 2 template yields the YANG schema DATA path — the structural
identity of the subscription target, independent of any specific instance.
Two subscriptions targeting the same schema node (one pinned to `eth0`, one
open) produce the same stripped path.

The topic name is therefore a character-level transformation of this schema
path. No additional schema resolution is needed.

## Why Information-Theoretic Optimization is Unsafe

An appealing optimization is to drop zero-entropy nodes — schema nodes with
a branching factor of 1 (exactly one child). Since traversing them adds zero
bits of information, they can theoretically be removed without losing
decodability. For example, `interfaces` (a wrapper container with one child
`interface`) would be dropped, shortening `if-interfaces-interface-mtu` to
`if-interface-mtu`.

This is unsafe under schema augmentation. YANG augmentation (RFC 7950 Section
7.17) allows an external module to add sibling nodes to any point in the
schema tree. A wrapper container that has branching factor 1 today may have
branching factor 2 tomorrow after an augmentation is loaded. At that point:

- The zero-entropy condition no longer holds.
- The topic name would need to change to include the previously-dropped node.
- Existing Kafka topics and consumer subscriptions would break silently.

The same argument applies to shortest-unique-prefix abbreviation. If
`interface` is today the only child starting with `i`, abbreviating to `i` is
correct. But an augmentation adding `inventory` would require changing the
abbreviation to `int`, breaking existing topics.

**Conclusion:** any optimization that depends on the current set of schema
siblings is unstable under augmentation and must not be used for topic names.
Topic names must be derivable from the path alone, without reference to what
other nodes exist at the same level.

## Algorithm

### Input

The Phase 2 key template for one branch of the subscription. This is
available without additional computation — Phase 2 already produced it.

Example: `/ietf-interfaces:interfaces/interface[name='%s']/mtu`

### Step 1: Strip Predicates

Remove all `[...]` predicate expressions from the template. This yields the
schema DATA path — the structural identity of the target.

`/ietf-interfaces:interfaces/interface/mtu`

Implementation: `strip_predicates()` in `src/xpath.rs` (already exists).

### Step 2: Replace Module Names with YANG Prefixes

Walk the path segments. Wherever a segment has a `module:name` prefix, look
up the module in the YANG context and substitute its `prefix` statement.

YANG module prefixes are:
- Short by convention (2-6 characters)
- Unique within any loaded schema context (enforced by libyang)
- Immutable once a module is published (RFC 7950 Section 4.1)

`/ietf-interfaces:interfaces/interface/mtu` becomes `/if:interfaces/interface/mtu`

Implementation: parse each `/`-separated segment, check for `:`, look up the
module prefix from the `Context`, replace.

### Step 3: Flatten to Topic Name

Apply three mechanical substitutions:

1. Remove the leading `/`.
2. Replace every `:` with `-`.
3. Replace every `/` with `-`.

`if:interfaces/interface/mtu` becomes `if-interfaces-interface-mtu`

All resulting characters (`[a-z0-9-]`) are valid in Kafka topic names.

### Step 4: Prepend Organization Prefix (Optional)

Some organizations require topic names to be scoped by team, project, or
organizational unit. When a prefix is configured, it is prepended with a `-`
separator:

`if-interfaces-interface-mtu` becomes `netops-if-interfaces-interface-mtu`

The prefix must contain only Kafka-safe characters (`[a-zA-Z0-9._-]`). It is
treated as opaque — the algorithm does not interpret it, just prepends it.

When no prefix is configured, this step is a no-op.

### Step 5: Handle Overflow

The maximum topic name length is configurable, defaulting to 255.  Deployments
with stricter limits can lower this value.

When a prefix is configured, it and its `-` separator consume part of the
length budget. The schema-derived portion is allocated the remaining
characters:

```
budget = max_length - len(prefix) - 1    (if prefix is set)
budget = max_length                       (if no prefix)
```

If the schema-derived portion exceeds its budget:

1. Compute FNV-1a (64-bit) of the full stripped schema path (before prefix
   substitution — the canonical form).
2. Take the first 8 hexadecimal characters of the hash.
3. Truncate the schema-derived portion at the last `-` boundary that keeps
   it within `budget - 1 - 8` characters.
4. Append `-` and the 8-character hash.
5. Prepend the organization prefix (if any).

The result is guaranteed to be at or below `max_length`.

If the prefix alone consumes the entire budget (leaving fewer than 10
characters for the schema portion), the algorithm rejects the configuration
with an error rather than producing unusable topic names.

In practice, overflow never triggers at the default max length. The longest
realistic YANG paths produce topic names of 50-80 characters.

### Step 6: Union Subscriptions

A subscription with multiple branches (`xpath1 | xpath2`) produces one topic
name per branch. Each branch is independently mapped through Steps 1-5.
The caller decides whether to publish each branch to its own topic or merge
them.

## Properties

**Deterministic.** The mapping is a pure function of the schema DATA path,
the YANG module prefixes, the optional organization prefix, and the configured
max length. Phase 2 already normalized all syntactic variants.

**Collision-free.** The mapping is injective (reversible): given a topic
name, you can reconstruct the exact schema path by reversing the
substitutions. Different schema paths always produce different topic names.
The organization prefix adds a namespace — two teams using the same YANG
modules get distinct topics.

**Stable.** The topic name depends only on the subscription's own path
segments and their module prefixes. It does not depend on what other nodes
exist at the same schema level. Augmenting the schema adds new paths (and
therefore new topic names) but never changes existing ones.

**Regex-friendly.** The `-` separator creates a natural hierarchy.
`^if-interfaces-interface-.*` matches all interface fields.
`^if-.*` matches everything from the `ietf-interfaces` module.

**Readable.** The YANG prefix identifies the module, and the node names
(unabbreviated) are the YANG local names that operators already know from
CLI and NETCONF.

## Examples

| Subscription target | Stripped schema path | Topic name |
|---|---|---|
| Interface list | `/ietf-interfaces:interfaces/interface` | `if-interfaces-interface` |
| Interface MTU | `/ietf-interfaces:interfaces/interface/mtu` | `if-interfaces-interface-mtu` |
| Interface status | `/ietf-interfaces:interfaces/interface/oper-status` | `if-interfaces-interface-oper-status` |
| System clock | `/ietf-system:system/clock` | `sys-system-clock` |
| DNS search domains | `/ietf-system:system/dns/search-domain` | `sys-system-dns-search-domain` |
| DNS server | `/ietf-system:system/dns/server` | `sys-system-dns-server` |
| Nested interface | `/example-network:network-instances/network-instance/interface` | `ni-network-instances-network-instance-interface` |
| Nested intf status | `/example-network:network-instances/network-instance/interface/status` | `ni-network-instances-network-instance-interface-status` |
| VLAN list | `/example-vlans:vlans/vlan` | `vlan-vlans-vlan` |
| Route | `/example-routes:routes/route` | `rt-routes-route` |
| ACL entry | `/example-acl:access-lists/access-list/access-list-entry` | `acl-access-lists-access-list-access-list-entry` |
| 3-level nesting | `/example-deep:root/level1/level2/level3` | `deep-root-level1-level2-level3` |

## Consumer Subscription Patterns

| Goal | Regex |
|---|---|
| All interface fields | `^if-interfaces-interface-.*` |
| All data from ietf-interfaces | `^if-.*` |
| All nested interface fields | `^ni-network-instances-network-instance-interface-.*` |
| All data from any module | `^(if\|sys\|ni\|acl)-.*` |
