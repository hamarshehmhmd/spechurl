//! Minimal JSON samples derived from OpenAPI schemas.
//!
//! Explicit values win (`const`, `example`, `default`, then `examples`, then
//! `enum`). Otherwise spechurl builds the smallest useful JSON value: required
//! object properties when any are listed, otherwise every property.

use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::spec::{self, Document, MediaType};

const MAX_DEPTH: usize = 16;

/// Sample a request-body or parameter schema.
pub(crate) fn sample_schema(
    schema: &Value,
    document: &Document,
    depth: usize,
    stack: &mut Vec<String>,
) -> Result<Value> {
    if !schema.is_object() {
        return Err(Error::spec(
            "schema must be an object (boolean schemas are not supported in v0.1)",
        ));
    }
    if depth > MAX_DEPTH {
        return Ok(Value::Null);
    }
    if let Some(value) = explicit_value(schema) {
        return Ok(value);
    }
    if let Some(value) = first_enum(schema) {
        return Ok(value);
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        if stack.iter().any(|seen| seen == reference) {
            return Ok(Value::Null);
        }
        let resolved = spec::pointer(document, reference, "schema")?;
        stack.push(reference.to_string());
        let sample = sample_schema(resolved, document, depth + 1, stack)?;
        stack.pop();
        return Ok(sample);
    }
    if let Some(sample) = sample_composition(schema, document, depth, stack)? {
        return Ok(sample);
    }
    sample_typed(schema, document, depth, stack)
}

fn explicit_value(schema: &Value) -> Option<Value> {
    for key in ["const", "example", "default"] {
        if let Some(value) = schema.get(key) {
            return Some(value.clone());
        }
    }
    let examples = schema.get("examples")?;
    match examples {
        Value::Array(items) => items.first().cloned(),
        _ => None,
    }
}

fn first_enum(schema: &Value) -> Option<Value> {
    schema
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|items| items.first().cloned())
}

fn sample_composition(
    schema: &Value,
    document: &Document,
    depth: usize,
    stack: &mut Vec<String>,
) -> Result<Option<Value>> {
    if let Some(branches) = schema.get("allOf").and_then(Value::as_array) {
        if branches.is_empty() {
            return Ok(None);
        }
        let mut merged = Map::new();
        let mut fallback = None;
        for branch in branches {
            let sample = sample_schema(branch, document, depth + 1, stack)?;
            match sample {
                Value::Object(fields) => {
                    for (key, value) in fields {
                        merged.insert(key, value);
                    }
                }
                other => fallback = Some(other),
            }
        }
        if schema.get("properties").is_some() {
            if let Value::Object(fields) = sample_typed(schema, document, depth, stack)? {
                for (key, value) in fields {
                    merged.insert(key, value);
                }
            }
        }
        if !merged.is_empty() {
            return Ok(Some(Value::Object(merged)));
        }
        return Ok(fallback);
    }
    if let Some(branches) = schema
        .get("oneOf")
        .or_else(|| schema.get("anyOf"))
        .and_then(Value::as_array)
    {
        if let Some(first) = branches.first() {
            return Ok(Some(sample_schema(first, document, depth + 1, stack)?));
        }
    }
    Ok(None)
}

fn sample_typed(
    schema: &Value,
    document: &Document,
    depth: usize,
    stack: &mut Vec<String>,
) -> Result<Value> {
    match primary_type(schema).as_deref() {
        Some("object") => sample_object(schema, document, depth, stack),
        Some("array") => sample_array(schema, document, depth, stack),
        Some("string") => Ok(Value::String(sample_string(schema))),
        Some("integer") => Ok(Value::Number(sample_integer(schema).into())),
        Some("number") => Ok(json_f64(sample_float(schema))),
        Some("boolean") => Ok(Value::Bool(true)),
        Some("null") => Ok(Value::Null),
        Some(_) => Ok(Value::String("string".to_string())),
        None if schema.get("properties").is_some()
            || schema.get("additionalProperties").is_some() =>
        {
            sample_object(schema, document, depth, stack)
        }
        None if schema.get("items").is_some() => sample_array(schema, document, depth, stack),
        None if schema.get("format").is_some()
            || schema.get("minLength").is_some()
            || schema.get("maxLength").is_some()
            || schema.get("pattern").is_some() =>
        {
            Ok(Value::String(sample_string(schema)))
        }
        None if schema.get("minimum").is_some() || schema.get("maximum").is_some() => {
            Ok(json_f64(sample_float(schema)))
        }
        None => Ok(Value::Object(Map::new())),
    }
}

fn sample_object(
    schema: &Value,
    document: &Document,
    depth: usize,
    stack: &mut Vec<String>,
) -> Result<Value> {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return Ok(Value::Object(Map::new()));
    };
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut keys: Vec<String> = if required.is_empty() {
        properties.keys().cloned().collect()
    } else {
        required
            .iter()
            .filter(|key| properties.contains_key(*key))
            .cloned()
            .collect()
    };
    keys.retain(|key| !schema_is_read_only(&properties[key], document, &mut Vec::new()));
    if keys.is_empty() {
        keys = properties
            .keys()
            .filter(|key| !schema_is_read_only(&properties[*key], document, &mut Vec::new()))
            .cloned()
            .collect();
    }
    keys.sort();

    let mut object = Map::new();
    for key in keys {
        let property = &properties[&key];
        let sample = sample_schema(property, document, depth + 1, stack)?;
        object.insert(key, sample);
    }
    for name in required {
        if !properties.contains_key(&name) && !object.contains_key(&name) {
            object.insert(name, Value::String("string".to_string()));
        }
    }
    Ok(Value::Object(object))
}

fn schema_is_read_only(schema: &Value, document: &Document, stack: &mut Vec<String>) -> bool {
    if schema
        .get("readOnly")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }
    let Some(reference) = schema.get("$ref").and_then(Value::as_str) else {
        return false;
    };
    if stack.iter().any(|seen| seen == reference) {
        return false;
    }
    let Ok(resolved) = spec::pointer(document, reference, "schema") else {
        return false;
    };
    stack.push(reference.to_string());
    let read_only = schema_is_read_only(resolved, document, stack);
    stack.pop();
    read_only
}

fn sample_array(
    schema: &Value,
    document: &Document,
    depth: usize,
    stack: &mut Vec<String>,
) -> Result<Value> {
    let max_items = schema.get("maxItems").and_then(Value::as_u64);
    if max_items == Some(0) {
        return Ok(Value::Array(Vec::new()));
    }
    let min_items = schema.get("minItems").and_then(Value::as_u64).unwrap_or(1);
    let count = usize::try_from(min_items.max(1)).unwrap_or(1).min(5);
    let Some(items) = schema.get("items") else {
        return Ok(Value::Array(Vec::new()));
    };
    if items.is_boolean() {
        return Ok(Value::Array(Vec::new()));
    }
    let item = sample_schema(items, document, depth + 1, stack)?;
    Ok(Value::Array(vec![item; count]))
}

fn sample_string(schema: &Value) -> String {
    let format = schema.get("format").and_then(Value::as_str);
    let mut text = match format {
        Some("date-time") => "2020-01-01T00:00:00Z".to_string(),
        Some("date") => "2020-01-01".to_string(),
        Some("uuid") => "00000000-0000-0000-0000-000000000000".to_string(),
        Some("email") => "user@example.com".to_string(),
        Some("uri" | "url" | "uri-reference") => "https://example.com".to_string(),
        Some("byte") => "YQ==".to_string(),
        Some("ipv4") => "127.0.0.1".to_string(),
        Some("ipv6") => "::1".to_string(),
        _ => "string".to_string(),
    };
    if let Some(min_length) = schema.get("minLength").and_then(Value::as_u64) {
        let min_length = usize::try_from(min_length).unwrap_or(usize::MAX);
        if text.chars().count() < min_length {
            text = "a".repeat(min_length);
        }
    }
    if let Some(max_length) = schema.get("maxLength").and_then(Value::as_u64) {
        let max_length = usize::try_from(max_length).unwrap_or(0);
        if text.chars().count() > max_length {
            text = text.chars().take(max_length).collect();
        }
    }
    text
}

fn sample_integer(schema: &Value) -> i64 {
    if let Some(minimum) = exclusive_minimum_number(schema) {
        return clamp_integer(minimum.floor() as i64 + 1, schema);
    }
    let mut value = schema
        .get("minimum")
        .and_then(Value::as_f64)
        .map(ceil_i64)
        .unwrap_or(0);
    if schema
        .get("exclusiveMinimum")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && schema.get("minimum").is_some()
    {
        value = value.saturating_add(1);
    }
    clamp_integer(value, schema)
}

fn sample_float(schema: &Value) -> f64 {
    if let Some(minimum) = exclusive_minimum_number(schema) {
        return minimum + 1.0;
    }
    let mut value = schema.get("minimum").and_then(Value::as_f64).unwrap_or(0.0);
    if schema
        .get("exclusiveMinimum")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && schema.get("minimum").is_some()
    {
        value += 1.0;
    }
    if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64) {
        if value > maximum {
            return maximum;
        }
    }
    value
}

fn exclusive_minimum_number(schema: &Value) -> Option<f64> {
    schema.get("exclusiveMinimum").and_then(Value::as_f64)
}

fn clamp_integer(value: i64, schema: &Value) -> i64 {
    if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64) {
        let maximum = maximum.floor() as i64;
        if value > maximum {
            return maximum;
        }
    }
    value
}

fn ceil_i64(value: f64) -> i64 {
    if !value.is_finite() {
        return 0;
    }
    let ceil = value.ceil();
    if ceil >= i64::MAX as f64 {
        i64::MAX
    } else if ceil <= i64::MIN as f64 {
        i64::MIN
    } else {
        ceil as i64
    }
}

fn json_f64(value: f64) -> Value {
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn primary_type(schema: &Value) -> Option<String> {
    let value = schema.get("type")?;
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .find(|item| *item != "null")
            .map(str::to_string)
            .or_else(|| items.first().and_then(Value::as_str).map(str::to_string)),
        _ => None,
    }
}

/// Choose the media type spechurl will turn into a request body.
pub(crate) fn choose_media(
    content: &std::collections::BTreeMap<String, MediaType>,
) -> Option<(&str, &MediaType)> {
    let mut entries: Vec<(&str, &MediaType)> = content
        .iter()
        .map(|(name, media)| (name.as_str(), media))
        .collect();
    entries.sort_by(|left, right| {
        media_rank(left.0)
            .cmp(&media_rank(right.0))
            .then_with(|| left.0.cmp(right.0))
    });
    entries.first().copied()
}

pub(crate) fn media_rank(content_type: &str) -> u8 {
    let base = base_content_type(content_type);
    if base == "application/json" || base.ends_with("+json") {
        0
    } else if base == "application/x-www-form-urlencoded" {
        1
    } else if base.starts_with("text/") || base == "application/xml" || base.ends_with("+xml") {
        2
    } else {
        3
    }
}

pub(crate) fn base_content_type(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
        .to_ascii_lowercase()
}

pub(crate) fn is_json_content_type(content_type: &str) -> bool {
    let base = base_content_type(content_type);
    base == "application/json" || base.ends_with("+json")
}

/// Build a JSON value from a media type, preferring examples over schemas.
pub(crate) fn sample_media(media: &MediaType, document: &Document) -> Result<Option<Value>> {
    if let Some(example) = &media.example {
        return Ok(Some(example.clone()));
    }
    if let Some(examples) = &media.examples {
        for example in examples.values() {
            if let Some(value) = example_value(document, example, &mut Vec::new())? {
                return Ok(Some(value));
            }
        }
    }
    if let Some(schema) = &media.schema {
        return Ok(Some(sample_schema(schema, document, 0, &mut Vec::new())?));
    }
    Ok(None)
}

fn example_value(
    document: &Document,
    example: &Value,
    stack: &mut Vec<String>,
) -> Result<Option<Value>> {
    if let Some(reference) = example.get("$ref").and_then(Value::as_str) {
        if stack.iter().any(|seen| seen == reference) {
            return Err(Error::spec(format!("cyclic $ref '{reference}'")));
        }
        let resolved = spec::pointer(document, reference, "example")?;
        stack.push(reference.to_string());
        let value = example_value(document, resolved, stack)?;
        stack.pop();
        return Ok(value);
    }
    if let Some(value) = example.get("value") {
        return Ok(Some(value.clone()));
    }
    Ok(None)
}

/// Whether an optional parameter carries an author-supplied sample.
pub(crate) fn has_explicit_sample(
    schema: Option<&Value>,
    example: Option<&Value>,
    content: Option<&std::collections::BTreeMap<String, MediaType>>,
) -> bool {
    if example.is_some() {
        return true;
    }
    if let Some(schema) = schema {
        if schema.get("const").is_some()
            || schema.get("example").is_some()
            || schema.get("default").is_some()
        {
            return true;
        }
        if schema
            .get("examples")
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty())
        {
            return true;
        }
    }
    if let Some(content) = content {
        for media in content.values() {
            if media.example.is_some() {
                return true;
            }
            if media
                .examples
                .as_ref()
                .is_some_and(|examples| !examples.is_empty())
            {
                return true;
            }
        }
    }
    false
}

/// Render a JSON value the way it should appear in a URL, header, or query string.
pub(crate) fn value_to_wire_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        Value::Array(items)
            if items
                .iter()
                .all(|item| !item.is_object() && !item.is_array()) =>
        {
            items
                .iter()
                .map(value_to_wire_string)
                .collect::<Vec<_>>()
                .join(",")
        }
        other => serde_json::to_string(other).unwrap_or_else(|_| "null".to_string()),
    }
}
