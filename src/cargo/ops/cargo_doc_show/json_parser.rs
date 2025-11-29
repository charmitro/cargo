//! Parser for rustdoc JSON output format.
//!
//! Uses flexible parsing with serde_json::Value to handle the evolving rustdoc JSON format.

use crate::util::CargoResult;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Parsed rustdoc JSON crate data.
#[derive(Debug)]
pub struct Crate {
    pub root: String,
    pub crate_version: Option<String>,
    pub index: HashMap<String, Item>,
    pub paths: HashMap<String, ItemSummary>,
}

/// An item in the documentation.
#[derive(Debug)]
pub struct Item {
    pub id: String,
    pub name: Option<String>,
    pub docs: Option<String>,
    pub inner: ItemKind,
    pub deprecation: Option<Deprecation>,
}

/// Summary of an item's path.
#[derive(Debug)]
pub struct ItemSummary {
    pub path: Vec<String>,
}

/// Deprecation information.
#[derive(Debug)]
pub struct Deprecation {
    pub since: Option<String>,
    pub note: Option<String>,
}

/// The kind of item with its data.
#[derive(Debug)]
pub enum ItemKind {
    Module { items: Vec<String> },
    Function(FunctionData),
    Struct(StructData),
    Enum(EnumData),
    Trait(TraitData),
    Impl(ImplData),
    Variant(VariantData),
    StructField(String), // type as string
    TypeAlias { generics: String },
    Constant { type_: String },
    Static { type_: String, is_mutable: bool },
    AssocConst { type_: String },
    AssocType { type_: Option<String> },
    Other, // unknown item types
}

#[derive(Debug)]
pub struct FunctionData {
    pub sig: String,
    pub generics: String,
    pub is_const: bool,
    pub is_async: bool,
    pub is_unsafe: bool,
}

#[derive(Debug)]
pub struct StructData {
    pub kind: StructKind,
    pub generics: String,
    pub impls: Vec<String>,
}

#[derive(Debug)]
pub enum StructKind {
    Unit,
    Tuple,
    Plain { fields: Vec<String> },
}

#[derive(Debug)]
pub struct EnumData {
    pub variants: Vec<String>,
    pub generics: String,
    pub impls: Vec<String>,
}

#[derive(Debug)]
pub struct TraitData {
    pub items: Vec<String>,
    pub generics: String,
    pub is_unsafe: bool,
}

#[derive(Debug)]
pub struct ImplData {
    pub items: Vec<String>,
    pub trait_: Option<String>,
}

#[derive(Debug)]
pub struct VariantData {
    pub kind: VariantKind,
}

#[derive(Debug)]
pub enum VariantKind {
    Plain,
    Tuple(Vec<String>),
    Struct { fields: Vec<String> },
}

/// Parse a rustdoc JSON file.
pub fn parse_json(path: &Path) -> CargoResult<Crate> {
    let content = std::fs::read_to_string(path)?;
    let json: Value = serde_json::from_str(&content)?;

    let root = get_id(&json["root"]);
    let crate_version = json["crate_version"].as_str().map(|s| s.to_string());

    let mut index = HashMap::new();
    if let Some(idx) = json["index"].as_object() {
        for (id, item_json) in idx {
            if let Some(item) = parse_item(id, item_json) {
                index.insert(id.clone(), item);
            }
        }
    }

    let mut paths = HashMap::new();
    if let Some(p) = json["paths"].as_object() {
        for (id, summary_json) in p {
            if let Some(summary) = parse_item_summary(summary_json) {
                paths.insert(id.clone(), summary);
            }
        }
    }

    Ok(Crate {
        root,
        crate_version,
        index,
        paths,
    })
}

fn get_id(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn get_ids(v: &Value) -> Vec<String> {
    match v {
        Value::Array(arr) => arr.iter().map(get_id).collect(),
        _ => Vec::new(),
    }
}

fn parse_item(id: &str, json: &Value) -> Option<Item> {
    let name = json["name"].as_str().map(|s| s.to_string());
    let docs = json["docs"].as_str().map(|s| s.to_string());
    let deprecation = parse_deprecation(&json["deprecation"]);
    let inner = parse_item_kind(&json["inner"]);

    Some(Item {
        id: id.to_string(),
        name,
        docs,
        inner,
        deprecation,
    })
}

fn parse_deprecation(json: &Value) -> Option<Deprecation> {
    if json.is_null() {
        return None;
    }
    Some(Deprecation {
        since: json["since"].as_str().map(|s| s.to_string()),
        note: json["note"].as_str().map(|s| s.to_string()),
    })
}

fn parse_item_kind(json: &Value) -> ItemKind {
    // rustdoc JSON uses {"kind_name": {data}} format
    if let Some(obj) = json.as_object() {
        if let Some((kind, data)) = obj.iter().next() {
            return match kind.as_str() {
                "module" => ItemKind::Module {
                    items: get_ids(&data["items"]),
                },
                "function" => ItemKind::Function(parse_function_data(data)),
                "struct" => ItemKind::Struct(parse_struct_data(data)),
                "enum" => ItemKind::Enum(parse_enum_data(data)),
                "trait" => ItemKind::Trait(parse_trait_data(data)),
                "impl" => ItemKind::Impl(parse_impl_data(data)),
                "variant" => ItemKind::Variant(parse_variant_data(data)),
                "struct_field" => ItemKind::StructField(format_type(data)),
                "type_alias" => ItemKind::TypeAlias {
                    generics: format_generics(&data["generics"]),
                },
                "constant" => ItemKind::Constant {
                    type_: format_type(&data["type"]),
                },
                "static" => ItemKind::Static {
                    type_: format_type(&data["type"]),
                    is_mutable: data["is_mutable"].as_bool().unwrap_or(false),
                },
                "assoc_const" => ItemKind::AssocConst {
                    type_: format_type(&data["type"]),
                },
                "assoc_type" => ItemKind::AssocType {
                    type_: if data["type"].is_null() {
                        None
                    } else {
                        Some(format_type(&data["type"]))
                    },
                },
                _ => ItemKind::Other,
            };
        }
    }
    ItemKind::Other
}

fn parse_function_data(json: &Value) -> FunctionData {
    let sig = format_fn_sig(&json["sig"]);
    let generics = format_generics(&json["generics"]);
    let header = &json["header"];

    FunctionData {
        sig,
        generics,
        is_const: header["is_const"].as_bool().unwrap_or(false),
        is_async: header["is_async"].as_bool().unwrap_or(false),
        is_unsafe: header["is_unsafe"].as_bool().unwrap_or(false),
    }
}

fn parse_struct_data(json: &Value) -> StructData {
    let generics = format_generics(&json["generics"]);
    let impls = get_ids(&json["impls"]);

    let kind = if let Some(kind_obj) = json["kind"].as_object() {
        if kind_obj.contains_key("unit") {
            StructKind::Unit
        } else if kind_obj.contains_key("tuple") {
            StructKind::Tuple
        } else if let Some(plain_data) = kind_obj.get("plain") {
            StructKind::Plain {
                fields: get_ids(&plain_data["fields"]),
            }
        } else {
            StructKind::Unit
        }
    } else {
        StructKind::Unit
    };

    StructData {
        kind,
        generics,
        impls,
    }
}

fn parse_enum_data(json: &Value) -> EnumData {
    EnumData {
        variants: get_ids(&json["variants"]),
        generics: format_generics(&json["generics"]),
        impls: get_ids(&json["impls"]),
    }
}

fn parse_trait_data(json: &Value) -> TraitData {
    TraitData {
        items: get_ids(&json["items"]),
        generics: format_generics(&json["generics"]),
        is_unsafe: json["is_unsafe"].as_bool().unwrap_or(false),
    }
}

fn parse_impl_data(json: &Value) -> ImplData {
    let trait_ = if json["trait"].is_null() {
        None
    } else {
        Some(format_path(&json["trait"]))
    };

    ImplData {
        items: get_ids(&json["items"]),
        trait_,
    }
}

fn parse_variant_data(json: &Value) -> VariantData {
    let kind = if let Some(kind_obj) = json["kind"].as_object() {
        if kind_obj.contains_key("plain") {
            VariantKind::Plain
        } else if let Some(tuple_data) = kind_obj.get("tuple") {
            VariantKind::Tuple(get_ids(tuple_data))
        } else if let Some(struct_data) = kind_obj.get("struct") {
            VariantKind::Struct {
                fields: get_ids(&struct_data["fields"]),
            }
        } else {
            VariantKind::Plain
        }
    } else {
        VariantKind::Plain
    };

    VariantData { kind }
}

fn parse_item_summary(json: &Value) -> Option<ItemSummary> {
    let path: Vec<String> = json["path"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Some(ItemSummary { path })
}

/// Format a function signature from JSON.
fn format_fn_sig(json: &Value) -> String {
    let mut parts = Vec::new();

    if let Some(inputs) = json["inputs"].as_array() {
        for input in inputs {
            if let Some(arr) = input.as_array() {
                if arr.len() >= 2 {
                    let name = arr[0].as_str().unwrap_or("_");
                    let type_ = format_type(&arr[1]);
                    if name == "self" {
                        parts.push(format_self_param(&arr[1]));
                    } else {
                        parts.push(format!("{}: {}", name, type_));
                    }
                }
            }
        }
    }

    let params = parts.join(", ");
    let output = if json["output"].is_null() {
        String::new()
    } else {
        format!(" -> {}", format_type(&json["output"]))
    };

    format!("({}){}", params, output)
}

fn format_self_param(json: &Value) -> String {
    if let Some(obj) = json.as_object() {
        if let Some(bref) = obj.get("borrowed_ref") {
            let is_mut = bref["is_mutable"].as_bool().unwrap_or(false);
            let lifetime = bref["lifetime"]
                .as_str()
                .map(|l| format!("'{} ", l))
                .unwrap_or_default();
            return if is_mut {
                format!("&{}mut self", lifetime)
            } else {
                format!("&{}self", lifetime)
            };
        }
    }
    "self".to_string()
}

/// Format generics from JSON.
fn format_generics(json: &Value) -> String {
    if let Some(params) = json["params"].as_array() {
        if params.is_empty() {
            return String::new();
        }
        let formatted: Vec<String> = params
            .iter()
            .map(|p| {
                let name = p["name"].as_str().unwrap_or("_");
                if let Some(kind) = p["kind"].as_object() {
                    if kind.contains_key("lifetime") {
                        format!("'{}", name)
                    } else if kind.contains_key("const") {
                        if let Some(c) = kind.get("const") {
                            format!("const {}: {}", name, format_type(&c["type"]))
                        } else {
                            name.to_string()
                        }
                    } else {
                        name.to_string()
                    }
                } else {
                    name.to_string()
                }
            })
            .collect();
        format!("<{}>", formatted.join(", "))
    } else {
        String::new()
    }
}

/// Format a type from JSON.
fn format_type(json: &Value) -> String {
    if let Some(obj) = json.as_object() {
        if let Some((kind, data)) = obj.iter().next() {
            return match kind.as_str() {
                "resolved_path" => format_path(json),
                "generic" => data.as_str().unwrap_or("T").to_string(),
                "primitive" => data.as_str().unwrap_or("_").to_string(),
                "borrowed_ref" => {
                    let lifetime = data["lifetime"]
                        .as_str()
                        .map(|l| format!("'{} ", l))
                        .unwrap_or_default();
                    let is_mut = data["is_mutable"].as_bool().unwrap_or(false);
                    let inner = format_type(&data["type"]);
                    if is_mut {
                        format!("&{}mut {}", lifetime, inner)
                    } else {
                        format!("&{}{}", lifetime, inner)
                    }
                }
                "tuple" => {
                    if let Some(arr) = data.as_array() {
                        let inner: Vec<String> = arr.iter().map(format_type).collect();
                        format!("({})", inner.join(", "))
                    } else {
                        "()".to_string()
                    }
                }
                "slice" => format!("[{}]", format_type(data)),
                "array" => {
                    let inner = format_type(&data["type"]);
                    let len = data["len"].as_str().unwrap_or("_");
                    format!("[{}; {}]", inner, len)
                }
                "raw_pointer" => {
                    let is_mut = data["is_mutable"].as_bool().unwrap_or(false);
                    let inner = format_type(&data["type"]);
                    if is_mut {
                        format!("*mut {}", inner)
                    } else {
                        format!("*const {}", inner)
                    }
                }
                "impl_trait" => "impl Trait".to_string(),
                "infer" => "_".to_string(),
                "function_pointer" => "fn(...)".to_string(),
                "dyn_trait" => "dyn Trait".to_string(),
                "qualified_path" => {
                    let name = data["name"].as_str().unwrap_or("_");
                    let self_type = format_type(&data["self_type"]);
                    format!("<{} as ...>::{}", self_type, name)
                }
                _ => format!("{{{}}}", kind),
            };
        }
    }
    // Handle simple string values
    if let Some(s) = json.as_str() {
        return s.to_string();
    }
    "_".to_string()
}

/// Format a path from JSON.
fn format_path(json: &Value) -> String {
    if let Some(obj) = json.as_object() {
        // Handle {"resolved_path": {"path": "...", ...}} format
        if let Some(resolved) = obj.get("resolved_path") {
            let name = resolved["path"]
                .as_str()
                .or_else(|| resolved["name"].as_str())
                .unwrap_or("_");
            return name.to_string();
        }
        // Handle direct {"path": "...", ...} format
        let name = obj
            .get("path")
            .and_then(|v| v.as_str())
            .or_else(|| obj.get("name").and_then(|v| v.as_str()))
            .unwrap_or("_");
        return name.to_string();
    }
    "_".to_string()
}
