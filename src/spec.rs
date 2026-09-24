//! OpenAPI document loading and reference resolution.
//!
//! spechurl reads the subset of OpenAPI 3.0 and 3.1 it needs to emit contract
//! tests: paths, operations, parameters, request bodies, and responses.
//! Schemas stay as JSON values so 3.0 and 3.1 keyword shapes can be sampled
//! without a second schema crate.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::error::{Error, Result};

/// A loaded OpenAPI document and the operations spechurl will render.
pub(crate) struct Document {
    /// File name only, used in generated comments so output is path-stable.
    pub source_name: String,
    pub openapi: String,
    pub title: String,
    pub version: String,
    pub servers: Vec<String>,
    pub has_webhooks: bool,
    pub doc: Value,
    pub operations: Vec<Operation>,
}

/// One HTTP operation, with `$ref`s already resolved.
#[derive(Debug, Clone)]
pub(crate) struct Operation {
    pub method: String,
    pub path: String,
    pub operation_id: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub deprecated: bool,
    pub parameters: Vec<Parameter>,
    pub request_body: Option<RequestBody>,
    pub responses: BTreeMap<String, Response>,
}

/// A parameter after `$ref` resolution.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Parameter {
    pub name: String,
    #[serde(rename = "in")]
    pub location: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub schema: Option<Value>,
    #[serde(default)]
    pub example: Option<Value>,
    #[serde(default)]
    pub content: Option<BTreeMap<String, MediaType>>,
}

/// A request body after `$ref` resolution.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RequestBody {
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub content: BTreeMap<String, MediaType>,
}

/// A response after `$ref` resolution.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Response {
    #[serde(default)]
    pub content: Option<BTreeMap<String, MediaType>>,
}

/// An OpenAPI media type. Examples and schemas are kept as JSON values.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MediaType {
    #[serde(default)]
    pub schema: Option<Value>,
    #[serde(default)]
    pub example: Option<Value>,
    #[serde(default)]
    pub examples: Option<BTreeMap<String, Value>>,
}

#[derive(Debug, Clone, Deserialize)]
struct Spec {
    #[serde(deserialize_with = "string_or_number")]
    openapi: String,
    info: Info,
    #[serde(default)]
    servers: Vec<Server>,
    paths: BTreeMap<String, PathItem>,
}

#[derive(Debug, Clone, Deserialize)]
struct Info {
    title: String,
    #[serde(deserialize_with = "string_or_number")]
    version: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Server {
    url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PathItem {
    #[serde(default, rename = "$ref")]
    reference: Option<String>,
    #[serde(default)]
    parameters: Vec<RefOr<Parameter>>,
    #[serde(default)]
    get: Option<RawOperation>,
    #[serde(default)]
    put: Option<RawOperation>,
    #[serde(default)]
    post: Option<RawOperation>,
    #[serde(default)]
    delete: Option<RawOperation>,
    #[serde(default)]
    options: Option<RawOperation>,
    #[serde(default)]
    head: Option<RawOperation>,
    #[serde(default)]
    patch: Option<RawOperation>,
    #[serde(default)]
    trace: Option<RawOperation>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawOperation {
    #[serde(default, rename = "operationId")]
    operation_id: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    deprecated: bool,
    #[serde(default)]
    parameters: Vec<RefOr<Parameter>>,
    #[serde(default, rename = "requestBody")]
    request_body: Option<RefOr<RequestBody>>,
    #[serde(default)]
    responses: BTreeMap<String, RefOr<Response>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RefOr<T> {
    Ref(RefObject),
    Item(T),
}

#[derive(Debug, Clone, Deserialize)]
struct RefObject {
    #[serde(rename = "$ref")]
    reference: String,
}

/// Load an OpenAPI document from disk.
pub(crate) fn load_path(path: &Path) -> Result<Document> {
    let bytes = fs::read(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let text = String::from_utf8(bytes).map_err(|_| {
        Error::parse(
            path,
            "file is not valid UTF-8 (OpenAPI documents must be UTF-8 YAML or JSON)",
        )
    })?;
    let source_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("openapi")
        .to_string();
    load_str(path, &source_name, &text)
}

/// Parse an OpenAPI document from a string.
///
/// `error_path` is used in diagnostics. `source_name` is the stable file name
/// written into generated Hurl comments and the suite README.
pub(crate) fn load_str(error_path: &Path, source_name: &str, text: &str) -> Result<Document> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(text).map_err(|err| Error::parse(error_path, err.to_string()))?;
    let doc = yaml_to_json(yaml_value).map_err(|message| Error::parse(error_path, message))?;
    if !doc.is_object() {
        return Err(Error::parse(
            error_path,
            "document must be a YAML or JSON object",
        ));
    }
    if doc.get("openapi").is_none() && doc.get("swagger").is_some() {
        return Err(Error::spec(
            "OpenAPI 2.0 (Swagger) documents are not supported; spechurl supports OpenAPI 3.0 and 3.1",
        ));
    }
    if doc.get("openapi").is_none() {
        return Err(Error::spec(
            "missing 'openapi' field (expected OpenAPI 3.0 or 3.1)",
        ));
    }
    match doc.get("paths") {
        None => return Err(Error::spec("missing 'paths' object")),
        Some(paths) if !paths.is_object() => {
            return Err(Error::spec("'paths' must be an object"));
        }
        Some(_) => {}
    }
    if !doc.get("info").is_some_and(Value::is_object) {
        return Err(Error::spec("missing 'info' object"));
    }
    // Deserialize the JSON form so numeric YAML keys (such as status `200:`) are strings.
    let spec: Spec = serde_json::from_value(doc.clone())
        .map_err(|err| Error::parse(error_path, err.to_string()))?;
    validate_version(&spec.openapi)?;
    if spec.paths.is_empty() {
        return Err(Error::spec(format!(
            "spec '{source_name}' contains no paths"
        )));
    }

    let mut document = Document {
        source_name: source_name.to_string(),
        openapi: spec.openapi,
        title: spec.info.title,
        version: spec.info.version,
        servers: spec.servers.into_iter().map(|server| server.url).collect(),
        has_webhooks: doc.get("webhooks").is_some(),
        doc,
        operations: Vec::new(),
    };
    document.operations = collect_operations(&document, &spec.paths)?;
    Ok(document)
}

fn validate_version(version: &str) -> Result<()> {
    let major = version.split('.').next().unwrap_or(version);
    if major == "3" {
        Ok(())
    } else {
        Err(Error::spec(format!(
            "unsupported OpenAPI version '{version}' (supported: 3.0 and 3.1)"
        )))
    }
}

fn collect_operations(
    document: &Document,
    paths: &BTreeMap<String, PathItem>,
) -> Result<Vec<Operation>> {
    let mut operations = Vec::new();
    for (path, item) in paths {
        let item = resolve_path_item(document, item, &mut Vec::new())
            .map_err(|err| err.in_context(&format!("path '{path}'")))?;
        let path_parameters = resolve_parameters(document, &item.parameters)
            .map_err(|err| err.in_context(&format!("path '{path}'")))?;
        for (method, raw) in item.methods() {
            let context = format!("{method} {path}");
            if raw.responses.is_empty() {
                return Err(Error::spec(format!(
                    "{context}: operation is missing responses"
                )));
            }
            let op_parameters = resolve_parameters(document, &raw.parameters)
                .map_err(|err| err.in_context(&context))?;
            let parameters = merge_parameters(path_parameters.clone(), op_parameters);
            validate_parameters(&context, path, &parameters)?;
            let request_body = match &raw.request_body {
                Some(body) => Some(
                    resolve_request_body(document, body, &mut Vec::new())
                        .map_err(|err| err.in_context(&format!("{context} request body")))?,
                ),
                None => None,
            };
            let mut responses = BTreeMap::new();
            for (status, response) in &raw.responses {
                let resolved = resolve_response(document, response, &mut Vec::new())
                    .map_err(|err| err.in_context(&format!("{context} response {status}")))?;
                responses.insert(status.clone(), resolved);
            }
            operations.push(Operation {
                method: method.to_string(),
                path: path.clone(),
                operation_id: raw.operation_id.clone(),
                summary: raw.summary.clone(),
                description: raw.description.clone(),
                tags: raw.tags.clone(),
                deprecated: raw.deprecated,
                parameters,
                request_body,
                responses,
            });
        }
    }
    Ok(operations)
}

fn validate_parameters(context: &str, path: &str, parameters: &[Parameter]) -> Result<()> {
    for parameter in parameters {
        match parameter.location.as_str() {
            "path" | "query" | "header" | "cookie" => {}
            other => {
                return Err(Error::spec(format!(
                    "{context}: parameter '{}' has unsupported location '{other}' (expected path, query, header, or cookie)",
                    parameter.name
                )));
            }
        }
        if parameter.name.is_empty() {
            return Err(Error::spec(format!(
                "{context}: a parameter is missing its name"
            )));
        }
        if parameter.location == "header" && !is_token(&parameter.name) {
            return Err(Error::spec(format!(
                "{context}: header parameter '{}' is not a valid HTTP header name",
                parameter.name
            )));
        }
    }
    let template_names = path_template_names(path)
        .map_err(|message| Error::spec(format!("{context}: {message}")))?;
    for name in &template_names {
        let documented = parameters
            .iter()
            .any(|parameter| parameter.location == "path" && parameter.name == *name);
        if !documented {
            return Err(Error::spec(format!(
                "{context}: path parameter '{{{name}}}' is not documented"
            )));
        }
    }
    for parameter in parameters
        .iter()
        .filter(|parameter| parameter.location == "path")
    {
        if !template_names.iter().any(|name| name == &parameter.name) {
            return Err(Error::spec(format!(
                "{context}: path parameter '{}' does not appear in the path template",
                parameter.name
            )));
        }
    }
    Ok(())
}

fn is_token(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(
                    ch,
                    '!' | '#'
                        | '$'
                        | '%'
                        | '&'
                        | '\''
                        | '*'
                        | '+'
                        | '-'
                        | '.'
                        | '^'
                        | '_'
                        | '`'
                        | '|'
                        | '~'
                )
        })
}

fn path_template_names(path: &str) -> std::result::Result<Vec<String>, String> {
    let mut names = Vec::new();
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            return Err(format!("unclosed path parameter in '{path}'"));
        };
        let name = &after[..end];
        if name.is_empty() {
            return Err(format!("empty path parameter in '{path}'"));
        }
        names.push(name.to_string());
        rest = &after[end + 1..];
    }
    Ok(names)
}

fn merge_parameters(
    path_parameters: Vec<Parameter>,
    operation_parameters: Vec<Parameter>,
) -> Vec<Parameter> {
    let mut merged: Vec<Parameter> = Vec::new();
    for parameter in path_parameters.into_iter().chain(operation_parameters) {
        if let Some(existing) = merged.iter_mut().find(|existing| {
            existing.location == parameter.location && existing.name == parameter.name
        }) {
            *existing = parameter;
        } else {
            merged.push(parameter);
        }
    }
    merged
}

fn resolve_path_item(
    document: &Document,
    item: &PathItem,
    stack: &mut Vec<String>,
) -> Result<PathItem> {
    let Some(reference) = &item.reference else {
        return Ok(item.clone());
    };
    if !item.methods().is_empty() || !item.parameters.is_empty() {
        return Err(Error::spec(format!(
            "path item $ref '{reference}' is combined with local operations or parameters; spechurl v0.1 does not merge those siblings"
        )));
    }
    if stack.iter().any(|seen| seen == reference) {
        return Err(Error::spec(format!("cyclic $ref '{reference}'")));
    }
    stack.push(reference.clone());
    let value = pointer(document, reference, "path item")?;
    let resolved: PathItem = serde_json::from_value(value.clone()).map_err(|err| {
        Error::spec(format!(
            "could not decode path item $ref '{reference}': {err}"
        ))
    })?;
    let resolved = resolve_path_item(document, &resolved, stack)?;
    stack.pop();
    Ok(resolved)
}

fn resolve_parameters(
    document: &Document,
    parameters: &[RefOr<Parameter>],
) -> Result<Vec<Parameter>> {
    let mut resolved = Vec::with_capacity(parameters.len());
    for parameter in parameters {
        resolved.push(resolve_parameter(document, parameter, &mut Vec::new())?);
    }
    Ok(resolved)
}

fn resolve_parameter(
    document: &Document,
    parameter: &RefOr<Parameter>,
    stack: &mut Vec<String>,
) -> Result<Parameter> {
    match parameter {
        RefOr::Item(parameter) => Ok(parameter.clone()),
        RefOr::Ref(reference) => {
            if stack.iter().any(|seen| seen == &reference.reference) {
                return Err(Error::spec(format!(
                    "cyclic $ref '{}'",
                    reference.reference
                )));
            }
            stack.push(reference.reference.clone());
            let value = pointer(document, &reference.reference, "parameter")?;
            let next: RefOr<Parameter> = serde_json::from_value(value.clone()).map_err(|err| {
                Error::spec(format!(
                    "could not decode parameter $ref '{}': {err}",
                    reference.reference
                ))
            })?;
            let parameter = resolve_parameter(document, &next, stack)?;
            stack.pop();
            Ok(parameter)
        }
    }
}

fn resolve_request_body(
    document: &Document,
    body: &RefOr<RequestBody>,
    stack: &mut Vec<String>,
) -> Result<RequestBody> {
    match body {
        RefOr::Item(body) => Ok(body.clone()),
        RefOr::Ref(reference) => {
            if stack.iter().any(|seen| seen == &reference.reference) {
                return Err(Error::spec(format!(
                    "cyclic $ref '{}'",
                    reference.reference
                )));
            }
            stack.push(reference.reference.clone());
            let value = pointer(document, &reference.reference, "request body")?;
            let next: RefOr<RequestBody> =
                serde_json::from_value(value.clone()).map_err(|err| {
                    Error::spec(format!(
                        "could not decode request body $ref '{}': {err}",
                        reference.reference
                    ))
                })?;
            let body = resolve_request_body(document, &next, stack)?;
            stack.pop();
            Ok(body)
        }
    }
}

fn resolve_response(
    document: &Document,
    response: &RefOr<Response>,
    stack: &mut Vec<String>,
) -> Result<Response> {
    match response {
        RefOr::Item(response) => Ok(response.clone()),
        RefOr::Ref(reference) => {
            if stack.iter().any(|seen| seen == &reference.reference) {
                return Err(Error::spec(format!(
                    "cyclic $ref '{}'",
                    reference.reference
                )));
            }
            stack.push(reference.reference.clone());
            let value = pointer(document, &reference.reference, "response")?;
            let next: RefOr<Response> = serde_json::from_value(value.clone()).map_err(|err| {
                Error::spec(format!(
                    "could not decode response $ref '{}': {err}",
                    reference.reference
                ))
            })?;
            let response = resolve_response(document, &next, stack)?;
            stack.pop();
            Ok(response)
        }
    }
}

pub(crate) fn pointer<'a>(
    document: &'a Document,
    reference: &str,
    what: &str,
) -> Result<&'a Value> {
    let Some(json_pointer) = reference.strip_prefix('#') else {
        return Err(Error::spec(format!(
            "external $ref '{reference}' is not supported in v0.1 (only local '#/...' pointers)"
        )));
    };
    if json_pointer.is_empty() {
        return Ok(&document.doc);
    }
    document.doc.pointer(json_pointer).ok_or_else(|| {
        Error::spec(format!(
            "unresolved $ref '{reference}' while reading {what}"
        ))
    })
}

impl PathItem {
    fn methods(&self) -> Vec<(&'static str, &RawOperation)> {
        let candidates = [
            ("GET", self.get.as_ref()),
            ("POST", self.post.as_ref()),
            ("PUT", self.put.as_ref()),
            ("PATCH", self.patch.as_ref()),
            ("DELETE", self.delete.as_ref()),
            ("HEAD", self.head.as_ref()),
            ("OPTIONS", self.options.as_ref()),
            ("TRACE", self.trace.as_ref()),
        ];
        candidates
            .into_iter()
            .filter_map(|(method, operation)| operation.map(|operation| (method, operation)))
            .collect()
    }
}

fn yaml_to_json(value: serde_yaml::Value) -> std::result::Result<Value, String> {
    match value {
        serde_yaml::Value::Null => Ok(Value::Null),
        serde_yaml::Value::Bool(flag) => Ok(Value::Bool(flag)),
        serde_yaml::Value::Number(number) => Ok(yaml_number(number)),
        serde_yaml::Value::String(text) => Ok(Value::String(text)),
        serde_yaml::Value::Sequence(items) => {
            let mut array = Vec::with_capacity(items.len());
            for item in items {
                array.push(yaml_to_json(item)?);
            }
            Ok(Value::Array(array))
        }
        serde_yaml::Value::Mapping(mapping) => {
            let mut object = serde_json::Map::new();
            for (key, value) in mapping {
                let key = yaml_key(key)?;
                object.insert(key, yaml_to_json(value)?);
            }
            Ok(Value::Object(object))
        }
        serde_yaml::Value::Tagged(tagged) => yaml_to_json(tagged.value),
    }
}

fn yaml_number(number: serde_yaml::Number) -> Value {
    if let Some(value) = number.as_i64() {
        Value::Number(value.into())
    } else if let Some(value) = number.as_u64() {
        Value::Number(value.into())
    } else if let Some(value) = number.as_f64() {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    } else {
        Value::Null
    }
}

fn yaml_key(key: serde_yaml::Value) -> std::result::Result<String, String> {
    match key {
        serde_yaml::Value::String(text) => Ok(text),
        serde_yaml::Value::Number(number) => Ok(match yaml_number(number) {
            Value::Number(number) => number.to_string(),
            other => other.to_string(),
        }),
        serde_yaml::Value::Bool(flag) => Ok(flag.to_string()),
        other => Err(format!("YAML mapping key must be a string (got {other:?})")),
    }
}

fn string_or_number<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Visitor;

    impl serde::de::Visitor<'_> for Visitor {
        type Value = String;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or number")
        }

        fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<String, E> {
            Ok(value.to_string())
        }

        fn visit_string<E: serde::de::Error>(
            self,
            value: String,
        ) -> std::result::Result<String, E> {
            Ok(value)
        }

        fn visit_i64<E: serde::de::Error>(self, value: i64) -> std::result::Result<String, E> {
            Ok(value.to_string())
        }

        fn visit_u64<E: serde::de::Error>(self, value: u64) -> std::result::Result<String, E> {
            Ok(value.to_string())
        }

        fn visit_f64<E: serde::de::Error>(self, value: f64) -> std::result::Result<String, E> {
            Ok(value.to_string())
        }
    }

    deserializer.deserialize_any(Visitor)
}

/// Concrete server URL from the spec, when one can be passed straight to Hurl.
pub(crate) fn server_hint(servers: &[String]) -> Option<String> {
    let url = servers.first()?.trim();
    if url.contains('{') {
        return None;
    }
    let trimmed = url.trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Some(trimmed.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_yaml_keys_and_integers_survive() {
        let yaml = "openapi: 3.0.3\ninfo:\n  title: T\n  version: 1\npaths:\n  /n:\n    get:\n      responses:\n        200:\n          description: ok\n        201:\n          description: created\n";
        let doc = load_str(Path::new("inline.yaml"), "inline.yaml", yaml).unwrap();
        assert_eq!(doc.version, "1");
        let statuses: Vec<_> = doc.operations[0].responses.keys().cloned().collect();
        assert_eq!(statuses, vec!["200".to_string(), "201".to_string()]);
    }
}
