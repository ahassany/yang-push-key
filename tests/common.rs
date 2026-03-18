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

//! Shared test helpers for integration tests.

use yang4::context::{Context, ContextFlags};
use yang4::data::{DataFormat, DataParserFlags, DataTree, DataValidationFlags};

/// Shorthand: `("module-name", &["feature", ...])`.
///
/// Pass an empty slice `&[]` for no features.
pub type ModuleSpec = (&'static str, &'static [&'static str]);

/// Build a libyang [`Context`] with the given YANG modules loaded.
///
/// Module content is resolved from the embedded static strings
/// compiled into the test binary.
pub fn create_ctx(modules: &[ModuleSpec]) -> Context {
    let mut ctx =
        Context::new(ContextFlags::NO_YANGLIBRARY).expect("Failed to create libyang context");
    ctx.set_searchdir("assets/yang")
        .expect("Failed to set search directory for yang library");

    for &(name, features) in modules {
        ctx.load_module(name, None, features)
            .unwrap_or_else(|e| panic!("Failed to load module '{}': {}", name, e));
    }
    ctx
}

/// Parse an XML data string into a libyang [`DataTree`].
///
/// Uses `NO_VALIDATION` flag (parse-only, no schema validation).
/// The input is null-terminated for the underlying C FFI layer.
#[allow(dead_code)]
pub fn parse_data<'a>(ctx: &'a Context, xml: &str) -> DataTree<'a> {
    DataTree::parse_string(
        ctx,
        xml,
        DataFormat::XML,
        DataParserFlags::NO_VALIDATION,
        DataValidationFlags::empty(),
    )
    .expect("Failed to parse XML data")
}
