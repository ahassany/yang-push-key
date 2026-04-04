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

//! CLI utility for the YANG Push to Kafka key derivation algorithm.
//!
//! Exposes each phase individually and a combined pipeline:
//!
//! ```text
//! yang-push-key phase1   Subtree filter XML  ->  XPath
//! yang-push-key phase2   XPath               ->  key template (JSON)
//! yang-push-key phase3   XPath + data XML    ->  Kafka key (JSON)
//! yang-push-key pipeline Subtree + data XML  ->  Kafka key (JSON, all phases)
//! ```
//!
//! ## Schema loading
//!
//! YANG modules can be specified in two ways:
//!
//! 1. **Individual modules** — `-m ietf-interfaces -m ietf-ip:feature1,feature2`
//! 2. **YANG Library (RFC 8525)** — `--yang-library yang-library.xml`
//!
//! Both require `--yang-dir <DIR>` pointing at the directory that
//! contains the `.yang` files.  The two modes can be combined.

use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};
use serde::Serialize;
use yang4::context::{Context, ContextFlags};

use yang_push_key::types::TargetType;
use yang_push_key::{DerivationResult, derive_templates, normalize_subtree, produce_message_key};

// =====================================================================
//  CLI argument definitions
// =====================================================================

/// Derive unique Kafka message keys from YANG Push notifications.
#[derive(Parser)]
#[command(name = "yang-push-key", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Phase 1: normalize a subtree filter (XML) into XPath expression(s).
    Phase1 {
        /// Path to the subtree filter XML file (use "-" for stdin).
        subtree_file: String,
        #[command(flatten)]
        yang: YangArgs,
    },

    /// Phase 2: derive key template(s) from a subscription XPath.
    ///
    /// Prints JSON with the template, extraction specs, and target type
    /// for each branch.
    Phase2 {
        /// The subscription XPath expression.
        xpath: String,
        #[command(flatten)]
        yang: YangArgs,
    },

    /// Phase 3: produce a Kafka key from a subscription XPath and
    /// notification data.
    ///
    /// Internally runs Phase 2 on the given XPath, then extracts
    /// key values from the data tree. Outputs compact JSON.
    Phase3 {
        /// Path to the notification data XML file (use "-" for stdin).
        data_file: String,
        /// The subscription XPath expression.
        #[arg(long)]
        xpath: String,
        /// Node name (managed device identifier, e.g. hostname).
        #[arg(long)]
        node_name: String,
        /// YANG Push subscription ID.
        #[arg(long)]
        sub_id: String,
        #[command(flatten)]
        yang: YangArgs,
    },

    /// Full pipeline: Phase 1 -> 2 -> 3 in one shot.
    ///
    /// Takes a subtree filter and notification data, produces the
    /// final Kafka key as compact JSON.
    Pipeline {
        /// Path to the notification data XML file (use "-" for stdin).
        data_file: String,
        /// Path to the subtree filter XML file.
        #[arg(long)]
        subtree: String,
        /// Node name (managed device identifier, e.g. hostname).
        #[arg(long)]
        node_name: String,
        /// YANG Push subscription ID.
        #[arg(long)]
        sub_id: String,
        #[command(flatten)]
        yang: YangArgs,
    },
}

/// Common YANG schema loading arguments shared by all subcommands.
///
/// Two mutually-complementary modes are supported:
///
/// 1. **Module list** (the original mode) — provide `--yang-dir` and
///    one or more `-m MODULE` flags.
/// 2. **YANG Library** — provide `--yang-library` (a
///    [RFC 8525](https://datatracker.ietf.org/doc/html/rfc8525) file)
///    and `--yang-dir` for the search path.  All modules, revisions,
///    and features described in the library are loaded automatically.
///
/// The two modes may be combined: the library is loaded first, then
/// any extra `-m` modules are loaded on top.
#[derive(clap::Args)]
struct YangArgs {
    /// Path to directory containing .yang module files.
    #[arg(long, value_name = "DIR")]
    yang_dir: PathBuf,

    /// YANG module to load. Repeat for each module.
    ///
    /// Format: NAME or NAME:FEATURE1,FEATURE2
    ///
    /// Examples:
    ///   -m ietf-interfaces
    ///   -m ietf-ip:ipv4-non-contiguous-netmasks,ipv6-privacy-autoconf
    #[arg(short = 'm', long = "module", value_name = "MODULE")]
    modules: Vec<String>,

    /// Path to a YANG Library file (RFC 8525, XML or JSON).
    ///
    /// When provided, the context is bootstrapped from this file so
    /// you don't have to enumerate every module with `-m`.
    /// Use `--yang-library-format` to override the auto-detected format.
    #[arg(long, value_name = "FILE")]
    yang_library: Option<PathBuf>,

    /// Data format of the YANG Library file.
    ///
    /// If omitted the format is inferred from the file extension
    /// (.xml → XML, .json → JSON).  Use this flag when the extension
    /// is ambiguous.
    #[arg(long, value_name = "FORMAT", value_parser = parse_data_format)]
    yang_library_format: Option<yang4::data::DataFormat>,
}

// =====================================================================
//  JSON output structures (Phase 2)
// =====================================================================

#[derive(Serialize)]
struct Phase2Output {
    subscription_xpath: String,
    branches: Vec<BranchOutput>,
}

#[derive(Serialize)]
struct BranchOutput {
    branch_index: usize,
    branch_xpath: String,
    key_template: String,
    target_type: String,
    extractions: Vec<ExtractionOutput>,
}

#[derive(Serialize)]
struct ExtractionOutput {
    placeholder_index: usize,
    extraction_xpath: String,
    key_leaf: String,
    list_module: String,
    list_name: String,
}

fn target_type_str(tt: TargetType) -> &'static str {
    match tt {
        TargetType::Container => "container",
        TargetType::List => "list",
        TargetType::Leaf => "leaf",
        TargetType::LeafList => "leaf-list",
    }
}

fn derivation_to_json(d: &DerivationResult) -> Phase2Output {
    Phase2Output {
        subscription_xpath: d.subscription_xpath.clone(),
        branches: d
            .branches
            .iter()
            .map(|b| BranchOutput {
                branch_index: b.branch_index,
                branch_xpath: b.branch_xpath.clone(),
                key_template: b.key_template.clone(),
                target_type: target_type_str(b.target_type).to_string(),
                extractions: b
                    .extractions
                    .iter()
                    .enumerate()
                    .map(|(i, e)| ExtractionOutput {
                        placeholder_index: i,
                        extraction_xpath: e.extraction_xpath.clone(),
                        key_leaf: e.key_leaf_name.clone(),
                        list_module: e.list_module.clone(),
                        list_name: e.list_name.clone(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

// =====================================================================
//  Helpers
// =====================================================================

/// Parse a data-format string ("xml" or "json") for clap.
fn parse_data_format(s: &str) -> Result<yang4::data::DataFormat, String> {
    match s.to_ascii_lowercase().as_str() {
        "xml" => Ok(yang4::data::DataFormat::XML),
        "json" => Ok(yang4::data::DataFormat::JSON),
        other => Err(format!(
            "unsupported YANG library format '{}' (expected 'xml' or 'json')",
            other
        )),
    }
}

/// Infer the [`DataFormat`] from a file extension.
fn infer_library_format(path: &std::path::Path) -> Result<yang4::data::DataFormat, String> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("xml") => Ok(yang4::data::DataFormat::XML),
        Some("json") => Ok(yang4::data::DataFormat::JSON),
        _ => Err(format!(
            "cannot infer YANG library format from '{}'; \
             use --yang-library-format xml|json",
            path.display()
        )),
    }
}

/// Read file contents, or stdin when path is "-".
fn read_input(path: &str) -> Result<String, String> {
    if path == "-" {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("failed to read stdin: {}", e))?;
        Ok(buf)
    } else {
        fs::read_to_string(path).map_err(|e| format!("failed to read '{}': {}", path, e))
    }
}

/// Parse a module spec "name" or "name:feat1,feat2" into (name, features).
fn parse_module_spec(spec: &str) -> (&str, Vec<&str>) {
    match spec.split_once(':') {
        Some((name, feats)) => (name, feats.split(',').collect()),
        None => (spec, vec![]),
    }
}

/// Create a libyang context, set the search directory, and load all
/// requested YANG modules.
///
/// When `--yang-library` is provided the context is created from the
/// RFC 8525 file (XML or JSON).  Any additional `-m` modules are
/// loaded on top.  When `--yang-library` is absent at least one `-m`
/// module must be supplied.
fn build_context(args: &YangArgs) -> Result<Context, String> {
    let mut ctx = if let Some(ref lib_path) = args.yang_library {
        // --- YANG Library mode ---
        let fmt = match args.yang_library_format {
            Some(f) => f,
            None => infer_library_format(lib_path)?,
        };
        Context::new_from_yang_library_file(lib_path, fmt, &args.yang_dir, ContextFlags::empty())
            .map_err(|e| {
                format!(
                    "failed to create context from YANG library '{}': {}",
                    lib_path.display(),
                    e
                )
            })?
    } else {
        // --- Module-list mode (original) ---
        if args.modules.is_empty() {
            return Err("either --yang-library or at least one -m/--module is required".into());
        }
        let mut ctx = Context::new(ContextFlags::NO_YANGLIBRARY)
            .map_err(|e| format!("failed to create libyang context: {}", e))?;
        ctx.set_searchdir(&args.yang_dir).map_err(|e| {
            format!(
                "failed to set search dir '{}': {}",
                args.yang_dir.display(),
                e
            )
        })?;
        ctx
    };

    // Load any explicitly requested modules (works in both modes).
    for spec in &args.modules {
        let (name, features) = parse_module_spec(spec);
        let feat_refs: Vec<&str> = features.to_vec();
        ctx.load_module(name, None, &feat_refs)
            .map_err(|e| format!("failed to load module '{}': {}", name, e))?;
    }

    Ok(ctx)
}

/// Parse XML data into a libyang data tree (null-terminated for C FFI).
fn parse_data_tree<'a>(ctx: &'a Context, xml: &str) -> Result<yang4::data::DataTree<'a>, String> {
    use yang4::data::{DataFormat, DataParserFlags, DataTree, DataValidationFlags};

    let mut data = xml.trim().as_bytes().to_vec();
    data.push(0);
    DataTree::parse_string(
        ctx,
        &data,
        DataFormat::XML,
        DataParserFlags::NO_VALIDATION,
        DataValidationFlags::empty(),
    )
    .map_err(|e| format!("failed to parse data XML: {}", e))
}

// =====================================================================
//  Subcommand handlers
// =====================================================================

fn run_phase1(subtree_file: &str, yang: &YangArgs) -> Result<(), String> {
    let ctx = build_context(yang)?;
    let xml = read_input(subtree_file)?;

    let xpath = normalize_subtree(&ctx, &xml)?;

    println!("{}", xpath);
    Ok(())
}

fn run_phase2(xpath: &str, yang: &YangArgs) -> Result<(), String> {
    let ctx = build_context(yang)?;

    let derivation = derive_templates(&ctx, xpath)?;

    let output = derivation_to_json(&derivation);
    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| format!("JSON serialization failed: {}", e))?;
    println!("{}", json);
    Ok(())
}

fn run_phase3(
    data_file: &str,
    xpath: &str,
    node_name: &str,
    sub_id: &str,
    yang: &YangArgs,
) -> Result<(), String> {
    let ctx = build_context(yang)?;
    let data_xml = read_input(data_file)?;

    let derivation = derive_templates(&ctx, xpath)?;
    let dtree = parse_data_tree(&ctx, &data_xml)?;
    let result = produce_message_key(&derivation, &dtree, node_name, sub_id)?;

    println!("{}", result.message_key);
    Ok(())
}

fn run_pipeline(
    data_file: &str,
    subtree_file: &str,
    node_name: &str,
    sub_id: &str,
    yang: &YangArgs,
) -> Result<(), String> {
    let ctx = build_context(yang)?;
    let subtree_xml = read_input(subtree_file)?;
    let data_xml = read_input(data_file)?;

    let xpath = normalize_subtree(&ctx, &subtree_xml)?;
    let derivation = derive_templates(&ctx, &xpath)?;
    let dtree = parse_data_tree(&ctx, &data_xml)?;
    let result = produce_message_key(&derivation, &dtree, node_name, sub_id)?;

    println!("{}", result.message_key);
    Ok(())
}

// =====================================================================
//  Entry point
// =====================================================================

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Command::Phase1 { subtree_file, yang } => run_phase1(subtree_file, yang),
        Command::Phase2 { xpath, yang } => run_phase2(xpath, yang),
        Command::Phase3 {
            data_file,
            xpath,
            node_name,
            sub_id,
            yang,
        } => run_phase3(data_file, xpath, node_name, sub_id, yang),
        Command::Pipeline {
            data_file,
            subtree,
            node_name,
            sub_id,
            yang,
        } => run_pipeline(data_file, subtree, node_name, sub_id, yang),
    };

    if let Err(msg) = result {
        eprintln!("error: {}", msg);
        process::exit(1);
    }
}
