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

//! XPath parsing utilities shared across phases.
//!
//! Provides functions for splitting union XPaths, parsing predicates,
//! escaping values for XPath string literals, and stripping predicates
//! from paths.

/// Split an XPath expression on top-level `|` operators.
///
/// A `|` inside square brackets, parentheses, or quoted strings is
/// **not** treated as a union separator.  Each resulting branch is
/// trimmed of leading/trailing whitespace.
///
/// # Examples
///
/// ```
/// # use yang_push_key::xpath::split_union;
/// let branches = split_union("/a:x/y | /b:p/q");
/// assert_eq!(branches, vec!["/a:x/y", "/b:p/q"]);
///
/// // '|' inside predicates is NOT a separator
/// let branches = split_union("/a:x[k='a|b']");
/// assert_eq!(branches, vec!["/a:x[k='a|b']"]);
/// ```
pub fn split_union(xpath: &str) -> Vec<String> {
    let mut branches = Vec::new();
    let mut current = String::new();
    let mut bracket_depth = 0i32;
    let mut paren_depth = 0i32;
    let mut in_squote = false;
    let mut in_dquote = false;

    for ch in xpath.chars() {
        if in_squote {
            if ch == '\'' {
                in_squote = false;
            }
            current.push(ch);
            continue;
        }
        if in_dquote {
            if ch == '"' {
                in_dquote = false;
            }
            current.push(ch);
            continue;
        }
        match ch {
            '\'' => {
                in_squote = true;
                current.push(ch);
            }
            '"' => {
                in_dquote = true;
                current.push(ch);
            }
            '[' => {
                bracket_depth += 1;
                current.push(ch);
            }
            ']' => {
                bracket_depth -= 1;
                current.push(ch);
            }
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth -= 1;
                current.push(ch);
            }
            '|' if bracket_depth == 0 && paren_depth == 0 => {
                let t = current.trim().to_string();
                if !t.is_empty() {
                    branches.push(t);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let t = current.trim().to_string();
    if !t.is_empty() {
        branches.push(t);
    }
    branches
}

/// A single key=value pair parsed from an XPath predicate.
#[derive(Debug)]
pub struct PredKv {
    /// Key leaf local name (module prefix stripped).
    pub key: String,
    /// The literal value from the predicate.
    pub value: String,
}

/// Result of parsing predicates from one XPath step.
#[derive(Debug)]
pub struct ParsedPredicates {
    /// Key=value pairs in document order.
    pub kvs: Vec<PredKv>,
    /// Whether a positional predicate like `[1]` was found.
    /// Positional predicates do **not** pin key values.
    pub has_positional: bool,
}

/// Parse predicate expressions from a string like `[k1='v1'][mod:k2="v2"][3]`.
///
/// - `[key='value']` and `[key="value"]` → key-value pair (prefix stripped).
/// - `[N]` (digits only) → positional predicate (flagged, not a key-value).
/// - `[.='value']` → key is `"."` (leaf-list self-value).
pub fn parse_predicates(preds: &str) -> ParsedPredicates {
    let mut kvs = Vec::new();
    let mut has_positional = false;
    let mut chars = preds.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch != '[' {
            chars.next();
            continue;
        }
        chars.next(); // skip '['

        // Skip whitespace
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }

        // Positional: starts with digit
        if chars.peek().is_some_and(|c| c.is_ascii_digit()) {
            has_positional = true;
            while chars.peek().is_some_and(|&c| c != ']') {
                chars.next();
            }
            if chars.peek() == Some(&']') {
                chars.next();
            }
            continue;
        }

        // Collect key name
        let mut key_raw = String::new();
        while chars
            .peek()
            .is_some_and(|&c| c != '=' && c != ']' && !c.is_whitespace())
        {
            key_raw.push(chars.next().unwrap());
        }
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }

        if chars.peek() != Some(&'=') {
            // Not a key=value predicate; skip to ']'
            while chars.peek().is_some_and(|&c| c != ']') {
                chars.next();
            }
            if chars.peek() == Some(&']') {
                chars.next();
            }
            continue;
        }
        chars.next(); // skip '='

        // Strip module prefix from key name
        let local_key = match key_raw.find(':') {
            Some(pos) => key_raw[pos + 1..].to_string(),
            None => key_raw,
        };

        // Skip whitespace before value
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }

        // Parse value (single-quoted, double-quoted, or bare)
        let mut value = String::new();
        match chars.peek() {
            Some(&'\'') | Some(&'"') => {
                let quote = chars.next().unwrap();
                while chars.peek().is_some_and(|&c| c != quote) {
                    value.push(chars.next().unwrap());
                }
                if chars.peek() == Some(&quote) {
                    chars.next();
                }
            }
            _ => {
                while chars.peek().is_some_and(|&c| c != ']') {
                    value.push(chars.next().unwrap());
                }
                value = value.trim().to_string();
            }
        }

        kvs.push(PredKv {
            key: local_key,
            value,
        });

        // Skip to ']'
        while chars.peek().is_some_and(|&c| c != ']') {
            chars.next();
        }
        if chars.peek() == Some(&']') {
            chars.next();
        }
    }

    ParsedPredicates {
        kvs,
        has_positional,
    }
}

/// A parsed XPath path step: name plus optional predicates.
#[derive(Debug)]
pub struct XPathStep {
    /// Local name (module prefix stripped).
    pub local_name: String,
    /// Parsed predicate key-value pairs.
    pub kvs: Vec<PredKv>,
    /// Whether a positional predicate `[N]` was found.
    pub has_positional: bool,
}

/// Parse an absolute XPath into individual steps with their predicates.
///
/// Input: `/mod:container/list[key='val']/leaf`
/// Output: three `XPathStep`s for container, list (with kv), leaf.
pub fn parse_xpath_steps(branch: &str) -> Vec<XPathStep> {
    let mut steps = Vec::new();
    let chars: Vec<char> = branch.chars().collect();
    let len = chars.len();
    let mut pos = 0;

    // Skip leading '/'
    if pos < len && chars[pos] == '/' {
        pos += 1;
    }

    while pos < len {
        let start = pos;
        let mut bracket_depth = 0;
        while pos < len {
            match chars[pos] {
                '[' => bracket_depth += 1,
                ']' => bracket_depth -= 1,
                '/' if bracket_depth == 0 => break,
                _ => {}
            }
            pos += 1;
        }

        let segment: String = chars[start..pos].iter().collect();
        let bracket_pos = segment.find('[');
        let (name_part, pred_part) = match bracket_pos {
            Some(bp) => (&segment[..bp], &segment[bp..]),
            None => (segment.as_str(), ""),
        };

        let local_name = match name_part.find(':') {
            Some(cp) => name_part[cp + 1..].to_string(),
            None => name_part.to_string(),
        };

        let parsed = parse_predicates(pred_part);

        steps.push(XPathStep {
            local_name,
            kvs: parsed.kvs,
            has_positional: parsed.has_positional,
        });

        if pos < len && chars[pos] == '/' {
            pos += 1;
        }
    }
    steps
}

/// Escape a string value for use inside an XPath predicate.
///
/// - No single quotes → wrap in `'...'`
/// - Contains single quotes but no double quotes → wrap in `"..."`
/// - Contains both → use `concat(...)` function
pub fn escape_xpath_value(value: &str) -> String {
    if !value.contains('\'') {
        format!("'{}'", value)
    } else if !value.contains('"') {
        format!("\"{}\"", value)
    } else {
        let mut parts = Vec::new();
        for (i, part) in value.split('\'').enumerate() {
            if i > 0 {
                parts.push("\"'\"".to_string());
            }
            if !part.is_empty() {
                parts.push(format!("'{}'", part));
            }
        }
        format!("concat({})", parts.join(","))
    }
}

/// Remove all predicates from an XPath, returning only the path steps.
///
/// Example: `/a:x/y[k='v']/z` → `/a:x/y/z`
pub fn strip_predicates(path: &str) -> String {
    let mut result = String::new();
    let mut depth = 0i32;
    for ch in path.chars() {
        match ch {
            '[' => depth += 1,
            ']' => depth -= 1,
            _ if depth == 0 => result.push(ch),
            _ => {}
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_simple_union() {
        let branches = split_union("/a:x/y | /b:p/q");
        assert_eq!(branches, vec!["/a:x/y", "/b:p/q"]);
    }

    #[test]
    fn split_preserves_predicate_pipe() {
        let branches = split_union("/a:x[k='a|b'] | /b:p");
        assert_eq!(branches.len(), 2);
        assert!(branches[0].contains("a|b"));
    }

    #[test]
    fn split_preserves_quoted_pipe() {
        let branches = split_union(r#"/a:x[k="a|b"]"#);
        assert_eq!(branches.len(), 1);
    }

    #[test]
    fn parse_single_predicate() {
        let p = parse_predicates("[name='eth0']");
        assert_eq!(p.kvs.len(), 1);
        assert_eq!(p.kvs[0].key, "name");
        assert_eq!(p.kvs[0].value, "eth0");
        assert!(!p.has_positional);
    }

    #[test]
    fn parse_positional() {
        let p = parse_predicates("[1]");
        assert_eq!(p.kvs.len(), 0);
        assert!(p.has_positional);
    }

    #[test]
    fn parse_strips_prefix() {
        let p = parse_predicates("[mod:name='eth0']");
        assert_eq!(p.kvs[0].key, "name");
    }

    #[test]
    fn escape_simple_value() {
        assert_eq!(escape_xpath_value("eth0"), "'eth0'");
    }

    #[test]
    fn escape_value_with_single_quote() {
        assert_eq!(escape_xpath_value("O'Brien"), "\"O'Brien\"");
    }

    #[test]
    fn strip_predicates_from_path() {
        assert_eq!(strip_predicates("/a:x/y[k='v']/z[j='w']"), "/a:x/y/z");
    }
}
