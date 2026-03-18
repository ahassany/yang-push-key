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

//! # YANG Push to Kafka Key Derivation
//!
//! Derives unique Apache Kafka message keys from YANG Push on-change
//! notifications ([RFC 8641]).  The algorithm operates in three phases:
//!
//! | Phase | Module     | Input                         | Output                     |
//! |-------|------------|-------------------------------|----------------------------|
//! | 1     | [`phase1`] | Subtree filter XML + YANG ctx | XPath expression(s)        |
//! | 2     | [`phase2`] | XPath + YANG ctx              | Key template + extractions |
//! | 3     | [`phase3`] | Data tree + Phase 2 result    | Kafka message key          |
//!
//! ## Quick start
//!
//! ```ignore
//! use yang_push_key::{phase1, phase2, phase3};
//!
//! // Phase 1 (optional — only if subscription uses subtree filters)
//! let xpath = phase1::normalize_subtree(&ctx, subtree_xml)?;
//!
//! // Phase 2 (once per subscription)
//! let derivation = phase2::derive_templates(&ctx, &xpath)?;
//!
//! // Phase 3 (per notification)
//! let result = phase3::produce_kafka_key(
//!     &derivation, &data_tree, "router-01", "1042",
//! )?;
//! println!("{}", result.kafka_key);
//! ```
//!
//! [RFC 8641]: https://www.rfc-editor.org/rfc/rfc8641

pub mod phase1;
pub mod phase2;
pub mod phase3;
pub mod types;
pub mod xpath;

// Re-export the main entry points for convenience.
pub use phase1::normalize_subtree;
pub use phase2::derive_templates;
pub use phase3::produce_kafka_key;
pub use types::*;
