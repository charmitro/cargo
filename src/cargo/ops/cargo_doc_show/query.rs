//! Item path lookup for rustdoc JSON.
//!
//! This module provides functionality to find items by their path
//! (e.g., "HashMap::insert" or "std::vec::Vec").

use super::json_parser::{
    Crate, EnumData, FunctionData, Item, ItemKind, StructData, StructKind, TraitData, VariantKind,
};
use crate::util::CargoResult;

/// Processed documentation ready for display.
#[derive(Debug)]
pub struct ItemDoc {
    /// The name of the item.
    pub name: String,
    /// The kind of item (e.g., "struct", "function", "trait").
    pub kind: String,
    /// The signature/declaration of the item.
    pub signature: String,
    /// The documentation string.
    pub docs: Option<String>,
    /// Deprecation notice if any.
    pub deprecation: Option<String>,
    /// Methods associated with this item (for structs, enums, traits).
    pub methods: Vec<MethodDoc>,
    /// Fields (for structs).
    pub fields: Vec<FieldDoc>,
    /// Variants (for enums).
    pub variants: Vec<VariantDoc>,
}

/// Documentation for a method.
#[derive(Debug)]
pub struct MethodDoc {
    /// The signature of the method.
    pub signature: String,
    /// Brief documentation (first line).
    pub brief: Option<String>,
}

/// Documentation for a field.
#[derive(Debug)]
pub struct FieldDoc {
    /// The name of the field.
    pub name: String,
    /// The type of the field.
    pub type_str: String,
    /// Documentation for the field.
    pub docs: Option<String>,
}

/// Documentation for an enum variant.
#[derive(Debug)]
pub struct VariantDoc {
    /// The signature (e.g., "Foo(i32)" or "Bar { x: i32 }").
    pub signature: String,
    /// Documentation for the variant.
    pub docs: Option<String>,
}

/// Find an item in the crate by its path.
///
/// If `path` is `None`, returns the crate root documentation.
/// If `path` is `Some`, searches for the item by path (e.g., "HashMap::insert").
pub fn find_item(crate_data: &Crate, path: Option<&str>) -> CargoResult<ItemDoc> {
    match path {
        None => find_crate_root(crate_data),
        Some(p) if p.is_empty() => find_crate_root(crate_data),
        Some(p) => find_by_path(crate_data, p),
    }
}

/// Find the crate root documentation.
fn find_crate_root(crate_data: &Crate) -> CargoResult<ItemDoc> {
    let root_item = crate_data
        .index
        .get(&crate_data.root)
        .ok_or_else(|| anyhow::anyhow!("crate root not found in index"))?;

    let name = root_item
        .name
        .clone()
        .unwrap_or_else(|| "crate".to_string());

    let items = match &root_item.inner {
        ItemKind::Module { items, .. } => items.clone(),
        _ => anyhow::bail!("crate root is not a module"),
    };

    // Collect public items in the module
    let mut methods = Vec::new();
    for item_id in &items {
        if let Some(item) = crate_data.index.get(item_id) {
            if let Some(item_name) = &item.name {
                let kind_str = get_item_kind_str(&item.inner);
                let brief = item.docs.as_ref().and_then(|d| first_line(d));
                methods.push(MethodDoc {
                    signature: format!("{} {}", kind_str, item_name),
                    brief,
                });
            }
        }
    }

    Ok(ItemDoc {
        name,
        kind: "crate".to_string(),
        signature: format!(
            "crate {}{}",
            root_item.name.as_deref().unwrap_or(""),
            crate_data
                .crate_version
                .as_ref()
                .map(|v| format!(" v{}", v))
                .unwrap_or_default()
        ),
        docs: root_item.docs.clone(),
        deprecation: root_item
            .deprecation
            .as_ref()
            .map(|d| format_deprecation(d)),
        methods,
        fields: Vec::new(),
        variants: Vec::new(),
    })
}

/// Find an item by its path (e.g., "HashMap::insert" or "my_module::MyStruct").
fn find_by_path(crate_data: &Crate, path: &str) -> CargoResult<ItemDoc> {
    let segments: Vec<&str> = path.split("::").collect();

    // Start from the crate root
    let mut current_id = crate_data.root.clone();
    let mut found_item: Option<&Item> = None;

    // Try to traverse the path
    for (i, segment) in segments.iter().enumerate() {
        let current_item = crate_data
            .index
            .get(&current_id)
            .ok_or_else(|| anyhow::anyhow!("item not found: {}", path))?;

        // Look for the segment in this item's children
        match &current_item.inner {
            ItemKind::Module { items, .. } => {
                let mut found = false;
                for item_id in items {
                    if let Some(item) = crate_data.index.get(item_id) {
                        if item.name.as_deref() == Some(*segment) {
                            current_id = item_id.clone();
                            found = true;
                            if i == segments.len() - 1 {
                                found_item = Some(item);
                            }
                            break;
                        }
                    }
                }
                if !found {
                    // Try to find in impls if this is the last segment
                    if i == segments.len() - 1 {
                        // Search all items for this name
                        return find_item_globally(crate_data, &segments);
                    }
                    anyhow::bail!("item '{}' not found in path '{}'", segment, path);
                }
            }
            ItemKind::Struct(s) => {
                // Look for methods in impls
                return find_method_in_type(crate_data, &s.impls, *segment);
            }
            ItemKind::Enum(e) => {
                // Could be a variant or a method
                // First check variants
                for variant_id in &e.variants {
                    if let Some(variant) = crate_data.index.get(variant_id) {
                        if variant.name.as_deref() == Some(*segment) {
                            return item_to_doc(crate_data, variant);
                        }
                    }
                }
                // Then check impls
                return find_method_in_type(crate_data, &e.impls, *segment);
            }
            ItemKind::Trait(t) => {
                // Look for items in the trait
                for item_id in &t.items {
                    if let Some(item) = crate_data.index.get(item_id) {
                        if item.name.as_deref() == Some(*segment) {
                            return item_to_doc(crate_data, item);
                        }
                    }
                }
                anyhow::bail!("item '{}' not found in trait", segment);
            }
            _ => {
                anyhow::bail!(
                    "cannot traverse into item type: {:?}",
                    get_item_kind_str(&current_item.inner)
                );
            }
        }
    }

    if let Some(item) = found_item {
        item_to_doc(crate_data, item)
    } else {
        anyhow::bail!("item not found: {}", path)
    }
}

/// Search globally for an item by name segments.
fn find_item_globally(crate_data: &Crate, segments: &[&str]) -> CargoResult<ItemDoc> {
    let target_name = segments
        .last()
        .ok_or_else(|| anyhow::anyhow!("empty path"))?;

    // Search through all items
    for item in crate_data.index.values() {
        if item.name.as_deref() == Some(*target_name) {
            // Check if the path matches
            if let Some(summary) = crate_data.paths.get(&item.id) {
                let path_matches = if segments.len() == 1 {
                    true
                } else {
                    // Check if path ends with our segments
                    let item_path: Vec<&str> = summary.path.iter().map(|s| s.as_str()).collect();
                    item_path.ends_with(segments)
                };

                if path_matches {
                    return item_to_doc(crate_data, item);
                }
            }
        }
    }

    anyhow::bail!("item not found: {}", segments.join("::"))
}

/// Find a method in a type's impl blocks.
fn find_method_in_type(
    crate_data: &Crate,
    impl_ids: &[String],
    method_name: &str,
) -> CargoResult<ItemDoc> {
    for impl_id in impl_ids {
        if let Some(impl_item) = crate_data.index.get(impl_id) {
            if let ItemKind::Impl(impl_data) = &impl_item.inner {
                for item_id in &impl_data.items {
                    if let Some(item) = crate_data.index.get(item_id) {
                        if item.name.as_deref() == Some(method_name) {
                            return item_to_doc(crate_data, item);
                        }
                    }
                }
            }
        }
    }

    anyhow::bail!("method '{}' not found", method_name)
}

/// Convert an Item to ItemDoc.
fn item_to_doc(crate_data: &Crate, item: &Item) -> CargoResult<ItemDoc> {
    let name = item.name.clone().unwrap_or_else(|| "<unnamed>".to_string());
    let kind = get_item_kind_str(&item.inner);
    let signature = format_signature(&name, &item.inner);
    let deprecation = item.deprecation.as_ref().map(|d| format_deprecation(d));

    let (methods, fields, variants) = collect_associated_items(crate_data, item);

    Ok(ItemDoc {
        name,
        kind: kind.to_string(),
        signature,
        docs: item.docs.clone(),
        deprecation,
        methods,
        fields,
        variants,
    })
}

/// Get the string representation of an item kind.
fn get_item_kind_str(kind: &ItemKind) -> &'static str {
    match kind {
        ItemKind::Module { .. } => "mod",
        ItemKind::Function(_) => "fn",
        ItemKind::Struct(_) => "struct",
        ItemKind::Enum(_) => "enum",
        ItemKind::Trait(_) => "trait",
        ItemKind::Impl(_) => "impl",
        ItemKind::Variant(_) => "variant",
        ItemKind::StructField(_) => "field",
        ItemKind::TypeAlias { .. } => "type",
        ItemKind::Constant { .. } => "const",
        ItemKind::Static { .. } => "static",
        ItemKind::AssocConst { .. } => "const",
        ItemKind::AssocType { .. } => "type",
        ItemKind::Other => "item",
    }
}

/// Format an item's signature.
fn format_signature(name: &str, kind: &ItemKind) -> String {
    match kind {
        ItemKind::Function(f) => format_function_signature(name, f),
        ItemKind::Struct(s) => format_struct_signature(name, s),
        ItemKind::Enum(e) => format_enum_signature(name, e),
        ItemKind::Trait(t) => format_trait_signature(name, t),
        ItemKind::TypeAlias { generics } => {
            format!("type {}{} = ...", name, generics)
        }
        ItemKind::Constant { type_ } => {
            format!("const {}: {}", name, type_)
        }
        ItemKind::Static { type_, is_mutable } => {
            let mutability = if *is_mutable { "mut " } else { "" };
            format!("static {}{}: {}", mutability, name, type_)
        }
        ItemKind::Module { .. } => format!("mod {}", name),
        ItemKind::Variant(v) => format_variant_signature(name, v),
        ItemKind::AssocConst { type_ } => {
            format!("const {}: {}", name, type_)
        }
        ItemKind::AssocType { type_ } => {
            if let Some(t) = type_ {
                format!("type {} = {}", name, t)
            } else {
                format!("type {}", name)
            }
        }
        _ => format!("{} {}", get_item_kind_str(kind), name),
    }
}

/// Format a function signature.
fn format_function_signature(name: &str, f: &FunctionData) -> String {
    let mut sig = String::new();

    if f.is_const {
        sig.push_str("const ");
    }
    if f.is_async {
        sig.push_str("async ");
    }
    if f.is_unsafe {
        sig.push_str("unsafe ");
    }

    sig.push_str("fn ");
    sig.push_str(name);
    sig.push_str(&f.generics);
    sig.push_str(&f.sig);

    sig
}

/// Format a struct signature.
fn format_struct_signature(name: &str, s: &StructData) -> String {
    match &s.kind {
        StructKind::Unit => format!("struct {}{}", name, s.generics),
        StructKind::Tuple => format!("struct {}{}(...)", name, s.generics),
        StructKind::Plain { .. } => format!("struct {}{} {{ ... }}", name, s.generics),
    }
}

/// Format an enum signature.
fn format_enum_signature(name: &str, e: &EnumData) -> String {
    format!("enum {}{} {{ ... }}", name, e.generics)
}

/// Format a trait signature.
fn format_trait_signature(name: &str, t: &TraitData) -> String {
    let mut sig = String::new();
    if t.is_unsafe {
        sig.push_str("unsafe ");
    }
    sig.push_str("trait ");
    sig.push_str(name);
    sig.push_str(&t.generics);
    sig.push_str(" { ... }");
    sig
}

/// Format a variant signature.
fn format_variant_signature(name: &str, v: &super::json_parser::VariantData) -> String {
    match &v.kind {
        VariantKind::Plain => name.to_string(),
        VariantKind::Tuple(fields) => {
            format!("{}(/* {} fields */)", name, fields.len())
        }
        VariantKind::Struct { fields } => {
            format!("{} {{ /* {} fields */ }}", name, fields.len())
        }
    }
}

/// Collect associated items (methods, fields, variants).
fn collect_associated_items(
    crate_data: &Crate,
    item: &Item,
) -> (Vec<MethodDoc>, Vec<FieldDoc>, Vec<VariantDoc>) {
    let mut methods = Vec::new();
    let mut fields = Vec::new();
    let mut variants = Vec::new();

    match &item.inner {
        ItemKind::Struct(s) => {
            // Collect fields
            if let StructKind::Plain { fields: field_ids } = &s.kind {
                for field_id in field_ids {
                    if let Some(field_item) = crate_data.index.get(field_id) {
                        if let ItemKind::StructField(ty) = &field_item.inner {
                            fields.push(FieldDoc {
                                name: field_item.name.clone().unwrap_or_default(),
                                type_str: ty.clone(),
                                docs: field_item.docs.clone(),
                            });
                        }
                    }
                }
            }

            // Collect methods from impls
            methods = collect_methods_from_impls(crate_data, &s.impls);
        }
        ItemKind::Enum(e) => {
            // Collect variants
            for variant_id in &e.variants {
                if let Some(variant_item) = crate_data.index.get(variant_id) {
                    if let ItemKind::Variant(v) = &variant_item.inner {
                        variants.push(VariantDoc {
                            signature: format_variant_signature(
                                variant_item.name.as_deref().unwrap_or(""),
                                v,
                            ),
                            docs: variant_item.docs.clone(),
                        });
                    }
                }
            }

            // Collect methods from impls
            methods = collect_methods_from_impls(crate_data, &e.impls);
        }
        ItemKind::Trait(t) => {
            // Collect trait items as methods
            for item_id in &t.items {
                if let Some(trait_item) = crate_data.index.get(item_id) {
                    if let Some(name) = &trait_item.name {
                        methods.push(MethodDoc {
                            signature: format_signature(name, &trait_item.inner),
                            brief: trait_item.docs.as_ref().and_then(|d| first_line(d)),
                        });
                    }
                }
            }
        }
        _ => {}
    }

    (methods, fields, variants)
}

/// Collect methods from impl blocks.
fn collect_methods_from_impls(crate_data: &Crate, impl_ids: &[String]) -> Vec<MethodDoc> {
    let mut methods = Vec::new();

    for impl_id in impl_ids {
        if let Some(impl_item) = crate_data.index.get(impl_id) {
            if let ItemKind::Impl(impl_data) = &impl_item.inner {
                // Skip trait impls for now (focus on inherent impls)
                if impl_data.trait_.is_some() {
                    continue;
                }

                for item_id in &impl_data.items {
                    if let Some(method_item) = crate_data.index.get(item_id) {
                        if let Some(name) = &method_item.name {
                            methods.push(MethodDoc {
                                signature: format_signature(name, &method_item.inner),
                                brief: method_item.docs.as_ref().and_then(|d| first_line(d)),
                            });
                        }
                    }
                }
            }
        }
    }

    methods
}

/// Format deprecation information.
fn format_deprecation(dep: &super::json_parser::Deprecation) -> String {
    let mut result = String::new();
    if let Some(since) = &dep.since {
        result.push_str(&format!("since {}", since));
    }
    if let Some(note) = &dep.note {
        if !result.is_empty() {
            result.push_str(": ");
        }
        result.push_str(note);
    }
    if result.is_empty() {
        result.push_str("deprecated");
    }
    result
}

/// Get the first line of a documentation string.
fn first_line(docs: &str) -> Option<String> {
    docs.lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.len() > 80 {
                format!("{}...", &trimmed[..77])
            } else {
                trimmed.to_string()
            }
        })
}
