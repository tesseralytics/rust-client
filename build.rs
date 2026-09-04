//! Build-time codegen: emit Rust response models from the vendored OpenAPI spec.
// Brand names and JSON keys read fine without markup; keep the prose clean.
#![allow(clippy::doc_markdown)]
//!
//! `openapi.json` is the single committed source of truth; the generated
//! `tessera_generated.rs` lives in `$OUT_DIR` and is never committed, so the
//! models can never drift from the spec.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

/// Map a scalar OpenAPI `type` string to its Rust equivalent.
fn scalar_type(t: &str, ctx: &str) -> String {
    match t {
        "string" => "String".to_string(),
        "integer" => "i64".to_string(),
        other => panic!("codegen: unsupported schema construct in {ctx}: `type` {other}"),
    }
}

/// Map an OpenAPI property schema to its base Rust type (no `Option` wrapper).
///
/// Handles `$ref`, `{"type": ...}` scalars, `{"type": "array", "items": ...}`,
/// and OpenAPI 3.1 nullability (`type` arrays containing `"null"`). Returns the
/// base type name and whether the schema is nullable.
fn map_type(schema: &serde_json::Value, ctx: &str) -> (String, bool) {
    if let Some(reference) = schema.get("$ref").and_then(serde_json::Value::as_str) {
        let name = reference
            .strip_prefix("#/components/schemas/")
            .unwrap_or_else(|| panic!("codegen: dangling `$ref` in {ctx}: {reference}"));
        return (name.to_string(), false);
    }
    match schema.get("type") {
        Some(serde_json::Value::String(t)) => (scalar_type(t, ctx), false),
        Some(serde_json::Value::Array(types)) => {
            // OpenAPI 3.1 nullability: `{"type": ["string", "null"]}`.
            let nullable = types.iter().any(|t| t.as_str() == Some("null"));
            let mut base = None;
            for t in types {
                let t = t.as_str().unwrap_or_else(|| {
                    panic!(
                        "codegen: unsupported schema construct in {ctx}: non-string `type` entry"
                    )
                });
                if t == "null" {
                    continue;
                }
                assert!(
                    base.replace(scalar_type(t, ctx)).is_none(),
                    "codegen: unsupported schema construct in {ctx}: multi-type union"
                );
            }
            let base = base.unwrap_or_else(|| {
                panic!("codegen: unsupported schema construct in {ctx}: null-only `type`")
            });
            (base, nullable)
        }
        _ => panic!("codegen: unsupported schema construct in {ctx}"),
    }
}

/// Map an OpenAPI schema (including `array`) to its base Rust type.
fn map_schema(schema: &serde_json::Value, ctx: &str) -> (String, bool) {
    if schema.get("type").and_then(serde_json::Value::as_str) == Some("array") {
        let items = schema.get("items").unwrap_or_else(|| {
            panic!("codegen: unsupported schema construct in {ctx}: array without `items`")
        });
        let (item, item_nullable) = map_type(items, ctx);
        assert!(
            !item_nullable,
            "codegen: unsupported schema construct in {ctx}: nullable array `items`"
        );
        return (format!("Vec<{item}>"), false);
    }
    map_type(schema, ctx)
}

/// Emit `///` doc lines: the description verbatim, or a `{name}.` fallback.
fn push_doc(out: &mut String, name: &str, description: Option<&str>, indent: &str) {
    match description {
        Some(text) => {
            for line in text.split('\n') {
                // A trailing backslash would line-continue the doc comment.
                let line = line.strip_suffix('\\').unwrap_or(line);
                let _ = writeln!(out, "{indent}/// {line}");
            }
        }
        None => {
            let _ = writeln!(out, "{indent}/// {name}.");
        }
    }
}

fn emit_struct(out: &mut String, name: &str, schema: &serde_json::Value) {
    let obj = schema.as_object().unwrap_or_else(|| {
        panic!("codegen: unsupported schema construct in {name}: schema is not an object")
    });
    assert_eq!(
        obj.get("type").and_then(serde_json::Value::as_str),
        Some("object"),
        "codegen: unsupported schema construct in {name}: only object schemas are supported"
    );
    let properties = obj.get("properties").and_then(serde_json::Value::as_object).unwrap_or_else(|| {
        panic!("codegen: unsupported schema construct in {name}: object schema without `properties`")
    });
    let required_set: Vec<&str> = obj
        .get("required")
        .and_then(serde_json::Value::as_array)
        .map(|items| items.iter().filter_map(serde_json::Value::as_str).collect())
        .unwrap_or_default();

    push_doc(
        out,
        name,
        schema
            .get("description")
            .and_then(serde_json::Value::as_str),
        "",
    );
    out.push_str("#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]\n");
    out.push_str("#[serde(rename_all = \"snake_case\")]\n");
    let _ = writeln!(out, "pub struct {name} {{");
    for (prop_name, prop_schema) in properties {
        let ctx = format!("{name}.{prop_name}");
        let (base, nullable) = map_schema(prop_schema, &ctx);
        let required_field = required_set.contains(&prop_name.as_str()) && !nullable;
        push_doc(
            out,
            prop_name,
            prop_schema
                .get("description")
                .and_then(serde_json::Value::as_str),
            "    ",
        );
        if required_field {
            let _ = writeln!(out, "    pub {prop_name}: {base},");
        } else {
            out.push_str("    #[serde(default, skip_serializing_if = \"Option::is_none\")]\n");
            let _ = writeln!(out, "    pub {prop_name}: Option<{base}>,");
        }
    }
    out.push_str("}\n\n");
}

fn main() {
    println!("cargo:rerun-if-changed=openapi.json");

    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| panic!("codegen: CARGO_MANIFEST_DIR unset")),
    );
    let spec_path = manifest_dir.join("openapi.json");
    let raw = fs::read_to_string(&spec_path)
        .unwrap_or_else(|e| panic!("codegen: cannot read {}: {e}", spec_path.display()));
    let spec: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("codegen: openapi.json is not valid JSON: {e}"));

    let schemas = spec
        .pointer("/components/schemas")
        .and_then(serde_json::Value::as_object)
        .unwrap_or_else(|| panic!("codegen: openapi.json has no `components.schemas` object"));

    let mut out = String::from("//@ generated by build.rs from openapi.json — do not edit.\n\n");
    for (name, schema) in schemas {
        emit_struct(&mut out, name, schema);
    }

    let out_dir = PathBuf::from(
        std::env::var("OUT_DIR").unwrap_or_else(|_| panic!("codegen: OUT_DIR unset")),
    );
    std::fs::write(out_dir.join("tessera_generated.rs"), out)
        .unwrap_or_else(|e| panic!("codegen: failed to write tessera_generated.rs: {e}"));
}
