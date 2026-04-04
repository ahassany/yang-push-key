//! Kafka topic name derivation from YANG Push subscriptions.
//!
//! Converts a Phase 2 key template into a deterministic, human-readable
//! Kafka topic name by replacing YANG module names with their short
//! `prefix` statements and flattening the path into a `-`-separated
//! string.
//!
//! # Optional organization prefix
//!
//! A `TopicConfig` can carry an optional prefix (team name, org name,
//! project code, etc.) that is prepended to every topic name. The prefix
//! is separated from the schema-derived portion by `-` and is accounted
//! for in the maximum length calculation.
//!
//! # Stability guarantee
//!
//! Topic names depend only on the subscription's own schema path, the
//! YANG module prefixes, and the configured prefix. They do not depend
//! on sibling nodes or branching factor, so augmenting the schema with
//! new nodes never changes existing topic names.
//!
//! # Overflow handling
//!
//! If the resulting name exceeds the configured maximum length (default
//! 255), it is truncated at a `-` boundary and an FNV-1a hash suffix
//! is appended for uniqueness.

use yang4::context::Context;

use crate::types::DerivationResult;
use crate::xpath::strip_predicates;

/// Default maximum topic name length.
pub const DEFAULT_MAX_TOPIC_LEN: usize = 255;
/// Length of the hex hash suffix used when truncating.
const HASH_SUFFIX_LEN: usize = 8;

/// Configuration for topic name derivation.
#[derive(Debug, Clone)]
pub struct TopicConfig {
    /// Optional prefix prepended to every topic name (e.g. `"netops"`,
    /// `"platform-team"`, `"acme-corp"`).
    ///
    /// When set, the topic name becomes `<prefix>-<schema-derived-name>`.
    /// Must contain only Kafka-safe characters: `[a-zA-Z0-9._-]`.
    pub prefix: Option<String>,

    /// Maximum allowed topic name length. Defaults to
    /// [`DEFAULT_MAX_TOPIC_LEN`] (255).
    ///
    /// Kafka's hard limit is 249 characters, but some deployments set
    /// lower limits. The configured value includes the prefix and its
    /// separator.
    pub max_length: usize,
}

impl Default for TopicConfig {
    fn default() -> Self {
        Self {
            prefix: None,
            max_length: DEFAULT_MAX_TOPIC_LEN,
        }
    }
}

impl TopicConfig {
    /// Create a config with no prefix and the default max length.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the organization/team prefix.
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        let p = prefix.into();
        self.prefix = if p.is_empty() { None } else { Some(p) };
        self
    }

    /// Set the maximum topic name length.
    pub fn with_max_length(mut self, max_length: usize) -> Self {
        self.max_length = max_length;
        self
    }

    /// Compute the budget available for the schema-derived portion of
    /// the topic name, after accounting for the prefix and its `-`
    /// separator.
    fn schema_budget(&self) -> usize {
        match &self.prefix {
            Some(p) => self.max_length.saturating_sub(p.len() + 1),
            None => self.max_length,
        }
    }
}

/// Result of topic name derivation for one subscription.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicNameResult {
    /// One topic name per union branch, in branch order.
    pub topic_names: Vec<String>,
}

/// Derive Kafka topic names from a Phase 2 derivation result.
///
/// Each union branch produces its own topic name. The caller decides
/// whether to publish branches to separate topics or merge them.
///
/// # Arguments
///
/// * `ctx` - libyang context with YANG modules loaded (used to look up
///   module prefixes).
/// * `derivation` - Phase 2 output containing key templates.
/// * `config` - Topic naming configuration (prefix, max length).
///
/// # Errors
///
/// Returns `Err` if a module name in the schema path cannot be found
/// in the context, or if the configured prefix consumes the entire
/// length budget.
pub fn derive_topic_names(
    ctx: &Context,
    derivation: &DerivationResult,
    config: &TopicConfig,
) -> Result<TopicNameResult, String> {
    if config.schema_budget() < HASH_SUFFIX_LEN + 2 {
        return Err(format!(
            "prefix '{}' is too long for max_length {} — no room for schema path",
            config.prefix.as_deref().unwrap_or(""),
            config.max_length,
        ));
    }

    let mut topic_names = Vec::new();

    for branch in &derivation.branches {
        let schema_path = strip_predicates(&branch.key_template);
        let topic = schema_path_to_topic(ctx, &schema_path, config)?;
        topic_names.push(topic);
    }

    Ok(TopicNameResult { topic_names })
}

/// Derive a Kafka topic name from a single schema DATA path.
///
/// This is the core transformation:
/// 1. Parse path segments.
/// 2. Replace module names with YANG prefixes.
/// 3. Join with `-`.
/// 4. Prepend the organization prefix (if configured).
/// 5. Truncate + hash if over the configured max length.
pub fn schema_path_to_topic(
    ctx: &Context,
    schema_path: &str,
    config: &TopicConfig,
) -> Result<String, String> {
    let segments: Vec<&str> = schema_path
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    let mut parts: Vec<String> = Vec::with_capacity(segments.len() + 1);

    for segment in &segments {
        if let Some(colon_pos) = segment.find(':') {
            let module_name = &segment[..colon_pos];
            let local_name = &segment[colon_pos + 1..];
            let prefix = resolve_module_prefix(ctx, module_name)?;
            parts.push(prefix);
            parts.push(local_name.to_string());
        } else {
            parts.push(segment.to_string());
        }
    }

    let schema_portion = parts.join("-");
    let budget = config.schema_budget();

    // Apply truncation if the schema portion exceeds the budget
    let fitted = if schema_portion.len() <= budget {
        schema_portion
    } else {
        truncate_with_hash(&schema_portion, schema_path, budget)
    };

    // Prepend the prefix
    match &config.prefix {
        Some(pfx) => Ok(format!("{}-{}", pfx, fitted)),
        None => Ok(fitted),
    }
}

/// Look up a YANG module's `prefix` statement by module name.
fn resolve_module_prefix(ctx: &Context, module_name: &str) -> Result<String, String> {
    ctx.get_module_latest(module_name)
        .map(|m| m.prefix().to_string())
        .ok_or_else(|| format!("cannot find module '{}' for prefix lookup", module_name))
}

/// Truncate a topic name and append a hash suffix for uniqueness.
///
/// Truncates at the last `-` boundary that keeps the total at or below
/// `budget`, then appends `-` and an 8-character hex hash of the
/// original schema path.
fn truncate_with_hash(raw_topic: &str, schema_path: &str, budget: usize) -> String {
    let hash = simple_hash(schema_path);
    let hash_hex = format!("{:016x}", hash);
    let suffix = &hash_hex[..HASH_SUFFIX_LEN];

    // Max prefix length: budget - 1 (dash) - HASH_SUFFIX_LEN
    let max_prefix = budget.saturating_sub(1 + HASH_SUFFIX_LEN);

    let truncated = &raw_topic[..max_prefix.min(raw_topic.len())];
    let cut_point = truncated.rfind('-').unwrap_or(truncated.len());

    format!("{}-{}", &raw_topic[..cut_point], suffix)
}

/// FNV-1a (64-bit) hash for topic name disambiguation.
///
/// Fast, good distribution, no external dependency. Not for security —
/// only needs to distinguish paths that truncate to the same prefix.
fn simple_hash(input: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in input.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic() {
        let a = simple_hash("/ietf-interfaces:interfaces/interface");
        let b = simple_hash("/ietf-interfaces:interfaces/interface");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_differs_for_different_inputs() {
        let a = simple_hash("/ietf-interfaces:interfaces/interface");
        let b = simple_hash("/ietf-system:system/clock");
        assert_ne!(a, b);
    }

    #[test]
    fn truncate_with_hash_respects_budget() {
        let long_topic = "a-".repeat(150);
        let schema_path = "/some/very/long/path";
        let result = truncate_with_hash(&long_topic, schema_path, 50);
        assert!(result.len() <= 50);
    }

    #[test]
    fn truncate_preserves_dash_boundary() {
        let long_topic = format!("{}-{}", "abcdef".repeat(30), "final");
        let schema_path = "/test";
        let result = truncate_with_hash(&long_topic, schema_path, 50);
        let last_dash = result.rfind('-').unwrap();
        let suffix = &result[last_dash + 1..];
        assert_eq!(suffix.len(), HASH_SUFFIX_LEN);
    }

    #[test]
    fn default_config_has_no_prefix_and_255_max() {
        let cfg = TopicConfig::default();
        assert_eq!(cfg.prefix, None);
        assert_eq!(cfg.max_length, 255);
        assert_eq!(cfg.schema_budget(), 255);
    }

    #[test]
    fn prefix_reduces_schema_budget() {
        let cfg = TopicConfig::new().with_prefix("netops");
        // "netops" = 6 chars + 1 separator = 7, budget = 255 - 7 = 248
        assert_eq!(cfg.schema_budget(), 248);
    }

    #[test]
    fn empty_prefix_treated_as_none() {
        let cfg = TopicConfig::new().with_prefix("");
        assert_eq!(cfg.prefix, None);
        assert_eq!(cfg.schema_budget(), 255);
    }
}
