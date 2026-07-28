use std::collections::BTreeMap;

use hyper::Method;
use serde::{Deserialize, Serialize};

/// Top-level OpenAPI 3.1 document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiDocument {
    pub openapi: String,
    pub info: Info,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<Server>>,
    pub paths: BTreeMap<String, PathItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<Components>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<Tag>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    pub title: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub get: Option<Operation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<Operation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub put: Option<Operation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete: Option<Operation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<Operation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<Operation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Operation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<Operation>,
    /// The HTTP QUERY method (RFC 10008) — safe, idempotent, body-carrying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<Operation>,
}

impl PathItem {
    pub fn insert(&mut self, method: &Method, op: Operation) {
        if *method == crate::query_method() {
            self.query = Some(op);
            return;
        }
        match *method {
            Method::GET => self.get = Some(op),
            Method::POST => self.post = Some(op),
            Method::PUT => self.put = Some(op),
            Method::DELETE => self.delete = Some(op),
            Method::PATCH => self.patch = Some(op),
            Method::HEAD => self.head = Some(op),
            Method::OPTIONS => self.options = Some(op),
            Method::TRACE => self.trace = Some(op),
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Operation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "operationId")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Parameter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "requestBody")]
    pub request_body: Option<RequestBody>,
    #[serde(default)]
    pub responses: BTreeMap<String, Response>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
    /// Extra top-level fields merged into the Operation object (OpenAPI
    /// `openapi_extra`). Serialized flat alongside the typed fields.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    #[serde(rename = "in")]
    pub location: ParameterLocation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterLocation {
    Query,
    Path,
    Header,
    Cookie,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestBody {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub content: BTreeMap<String, MediaType>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaType {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<BTreeMap<String, MediaType>>,
}

/// Schema can be either a `$ref` reference or an inline SchemaObject.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum Schema {
    Ref {
        #[serde(rename = "$ref")]
        ref_path: String,
    },
    Object(SchemaObject),
}

impl Schema {
    pub fn ref_path(path: &str) -> Self {
        Self::Ref { ref_path: path.to_string() }
    }

    pub fn object(obj: SchemaObject) -> Self {
        Self::Object(obj)
    }

    pub fn string() -> Self {
        Self::Object(SchemaObject::string())
    }

    pub fn integer() -> Self {
        Self::Object(SchemaObject::integer())
    }

    pub fn number() -> Self {
        Self::Object(SchemaObject::number())
    }

    pub fn boolean() -> Self {
        Self::Object(SchemaObject::boolean())
    }

    pub fn object_type() -> Self {
        Self::Object(SchemaObject::object())
    }

    pub fn array(items: Schema) -> Self {
        Self::Object(SchemaObject::array().with_items(items))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaObject {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub schema_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<BTreeMap<String, Schema>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "additionalProperties")]
    pub additional_properties: Option<Box<Schema>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[serde(rename = "enum")]
    pub enum_values: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nullable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "readOnly")]
    pub read_only: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "writeOnly")]
    pub write_only: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "minLength")]
    pub min_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "maxLength")]
    pub max_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

impl SchemaObject {
    pub fn string() -> Self {
        Self { schema_type: Some("string".to_string()), ..Default::default() }
    }

    pub fn integer() -> Self {
        Self { schema_type: Some("integer".to_string()), ..Default::default() }
    }

    pub fn number() -> Self {
        Self { schema_type: Some("number".to_string()), ..Default::default() }
    }

    pub fn boolean() -> Self {
        Self { schema_type: Some("boolean".to_string()), ..Default::default() }
    }

    pub fn object() -> Self {
        Self { schema_type: Some("object".to_string()), ..Default::default() }
    }

    pub fn array() -> Self {
        Self { schema_type: Some("array".to_string()), ..Default::default() }
    }

    pub fn with_items(mut self, items: Schema) -> Self {
        self.items = Some(Box::new(items));
        self
    }

    pub fn with_property(mut self, name: &str, prop: Schema) -> Self {
        self.properties.get_or_insert_with(BTreeMap::new).insert(name.to_string(), prop);
        self
    }

    pub fn with_required(mut self, field: &str) -> Self {
        self.required.push(field.to_string());
        self
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Components {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schemas: Option<BTreeMap<String, SchemaObject>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct OpenApiBuilder {
    info: Info,
    servers: Vec<Server>,
    paths: BTreeMap<String, PathItem>,
    schemas: BTreeMap<String, SchemaObject>,
    tags: Vec<Tag>,
}

impl OpenApiBuilder {
    pub fn new(title: &str, version: &str) -> Self {
        Self {
            info: Info {
                title: title.to_string(),
                version: version.to_string(),
                description: None,
            },
            servers: Vec::new(),
            paths: BTreeMap::new(),
            schemas: BTreeMap::new(),
            tags: Vec::new(),
        }
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.info.description = Some(desc.to_string());
        self
    }

    pub fn server(mut self, url: &str, description: Option<&str>) -> Self {
        self.servers
            .push(Server { url: url.to_string(), description: description.map(|s| s.to_string()) });
        self
    }

    pub fn tag(mut self, name: &str, description: Option<&str>) -> Self {
        self.tags
            .push(Tag { name: name.to_string(), description: description.map(|s| s.to_string()) });
        self
    }

    pub fn schema(mut self, name: &str, schema: SchemaObject) -> Self {
        self.schemas.insert(name.to_string(), schema);
        self
    }

    pub fn operation<P: AsRef<str>>(mut self, method: Method, path: P, op: Operation) -> Self {
        let path_str = path.as_ref().to_string();
        let entry = self.paths.entry(path_str).or_insert_with(|| PathItem {
            get: None,
            post: None,
            put: None,
            delete: None,
            patch: None,
            head: None,
            options: None,
            trace: None,
            query: None,
        });
        entry.insert(&method, op);
        self
    }

    pub fn build(self) -> OpenApiDocument {
        let components = if self.schemas.is_empty() {
            None
        } else {
            Some(Components { schemas: Some(self.schemas) })
        };

        let tags = if self.tags.is_empty() { None } else { Some(self.tags) };

        let servers = if self.servers.is_empty() { None } else { Some(self.servers) };

        OpenApiDocument {
            openapi: "3.1.0".to_string(),
            info: self.info,
            servers,
            paths: self.paths,
            components,
            tags,
        }
    }
}

// ---------------------------------------------------------------------------
// Route metadata collection (for native API integration)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RouteMeta {
    pub method: Method,
    pub path: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub request_body_schema: Option<serde_json::Value>,
    pub response_schema: Option<serde_json::Value>,
    pub deprecated: bool,
    /// When true, the generated operation is tagged `experimental`
    /// (used for recently-standardized methods such as HTTP QUERY).
    pub experimental: bool,
    /// Default success status code for the operation (e.g. 201). When `None`
    /// the generated OpenAPI uses `200`.
    pub status_code: Option<u16>,
    /// Additional responses to merge into the OpenAPI `responses` object
    /// (keyed by status code). Raw JSON as provided by the user.
    pub responses: Option<serde_json::Value>,
    /// Custom operation ID. When `None`, one is auto-generated from the
    /// method + path.
    pub operation_id: Option<String>,
    /// Arbitrary extra fields merged into the generated Operation object
    /// (OpenAPI `openapi_extra`).
    pub openapi_extra: Option<serde_json::Value>,
    /// When false, the operation is excluded from the generated OpenAPI spec
    /// (FastAPI `include_in_schema=False`).
    pub include_in_schema: bool,
}

#[derive(Debug, Clone, Default)]
pub struct OpenApiRegistry {
    routes: Vec<RouteMeta>,
}

impl OpenApiRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, meta: RouteMeta) {
        self.routes.push(meta);
    }

    pub fn generate(&self, title: &str, version: &str) -> OpenApiDocument {
        let mut builder = OpenApiBuilder::new(title, version)
            .server("http://localhost:8080", Some("Local development"))
            .tag("default", Some("Default endpoint group"));

        for route in &self.routes {
            let auto_operation_id = format!(
                "{}_{}",
                route.method.as_str().to_lowercase(),
                route.path.replace(['/', '{', '}', ':'], "_").trim_matches('_')
            );
            let mut op = Operation {
                summary: route.summary.clone(),
                description: route.description.clone(),
                operation_id: route.operation_id.clone().or(Some(auto_operation_id)),
                tags: {
                    let mut tags = route.tags.clone();
                    if route.experimental && !tags.iter().any(|t| t == "experimental") {
                        tags.push("experimental".to_string());
                    }
                    tags
                },
                parameters: Vec::new(),
                request_body: None,
                responses: BTreeMap::new(),
                deprecated: if route.deprecated { Some(true) } else { None },
                extensions: BTreeMap::new(),
            };

            // Skip routes excluded from the schema (FastAPI include_in_schema=False).
            if !route.include_in_schema {
                continue;
            }

            let mut path_params = Vec::new();
            for segment in route.path.split('/') {
                if segment.starts_with('{') && segment.ends_with('}') {
                    let name = &segment[1..segment.len() - 1];
                    path_params.push(name.to_string());
                } else if let Some(name) = segment.strip_prefix(':') {
                    path_params.push(name.to_string());
                }
            }

            for name in &path_params {
                op.parameters.push(Parameter {
                    name: name.clone(),
                    location: ParameterLocation::Path,
                    description: None,
                    required: true,
                    schema: Some(Schema::string()),
                });
            }

            let is_body_method = matches!(route.method, Method::POST | Method::PUT | Method::PATCH)
                || route.method == crate::query_method();
            if is_body_method {
                let body_schema = route.request_body_schema.as_ref().map(json_value_to_schema);

                let mut content = BTreeMap::new();
                content.insert("application/json".to_string(), MediaType { schema: body_schema });
                op.request_body = Some(RequestBody { description: None, content, required: true });
            }

            let mut responses = BTreeMap::new();
            let success_code = route.status_code.unwrap_or(200).to_string();
            responses.insert(
                success_code.clone(),
                Response {
                    description: "Successful response".to_string(),
                    content: Some({
                        let mut c = BTreeMap::new();
                        c.insert(
                            "application/json".to_string(),
                            MediaType {
                                schema: route.response_schema.as_ref().map(json_value_to_schema),
                            },
                        );
                        c
                    }),
                },
            );
            responses.insert(
                "400".to_string(),
                Response { description: "Bad request".to_string(), content: None },
            );
            responses.insert(
                "500".to_string(),
                Response { description: "Internal server error".to_string(), content: None },
            );
            op.responses = responses;

            // Merge user-provided `responses` into the generated Operation.
            if let Some(resp) = &route.responses {
                if let Some(map) = resp.as_object() {
                    for (code, val) in map {
                        if let Ok(parsed) = serde_json::from_value::<Response>(val.clone()) {
                            op.responses.insert(code.clone(), parsed);
                        }
                    }
                }
            }

            // Merge `openapi_extra` into the Operation extensions (flattened
            // top-level fields, mirroring FastAPI's `openapi_extra`).
            if let Some(extra) = &route.openapi_extra {
                if let Some(map) = extra.as_object() {
                    for (key, val) in map {
                        if key == "responses" || key == "operationId" {
                            continue;
                        }
                        op.extensions.insert(key.clone(), val.clone());
                    }
                }
            }

            builder = builder.operation(route.method.clone(), &route.path, op);
        }

        builder.build()
    }
}

fn json_value_to_schema(value: &serde_json::Value) -> Schema {
    match value {
        serde_json::Value::Null => Schema::Object(SchemaObject {
            schema_type: Some("null".to_string()),
            ..Default::default()
        }),
        serde_json::Value::Bool(_) => Schema::boolean(),
        serde_json::Value::Number(n) => {
            if n.is_f64() {
                Schema::number()
            } else {
                Schema::integer()
            }
        }
        serde_json::Value::String(_) => Schema::string(),
        serde_json::Value::Array(items) => {
            // Merge schemas of all array elements to handle heterogeneous arrays
            let item_schema = if items.is_empty() {
                None
            } else {
                // Use the first element's schema as the item type.
                // For heterogeneous arrays this is a best-effort approximation.
                Some(json_value_to_schema(&items[0]))
            };
            Schema::Object(SchemaObject {
                schema_type: Some("array".to_string()),
                items: item_schema.map(Box::new),
                ..Default::default()
            })
        }
        serde_json::Value::Object(map) => {
            let mut obj = SchemaObject::object();
            let mut required = Vec::new();
            for (key, val) in map {
                obj.properties
                    .get_or_insert_with(BTreeMap::new)
                    .insert(key.clone(), json_value_to_schema(val));
                if !val.is_null() {
                    required.push(key.clone());
                }
            }
            obj.required = required;
            Schema::Object(obj)
        }
    }
}

// ---------------------------------------------------------------------------
// Swagger UI HTML helper
// ---------------------------------------------------------------------------

const SWAGGER_UI_HTML: &str = r###"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>JustAPI — Swagger UI</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css">
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script>
    SwaggerUIBundle({
      url: '/openapi.json',
      dom_id: '#swagger-ui',
      presets: [
        SwaggerUIBundle.presets.apis,
        SwaggerUIBundle.SwaggerUIStandalonePreset
      ],
      layout: "BaseLayout"
    });
  </script>
</body>
</html>"###;

const REDOC_HTML: &str = r###"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>JustAPI — ReDoc</title>
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <link rel="stylesheet" href="https://fonts.googleapis.com/css?family=Roboto:300,400,700|Roboto+Mono">
  <style>
    body { margin: 0; padding: 0; }
  </style>
</head>
<body>
  <div id="redoc-container"></div>
  <script src="https://cdn.redoc.ly/redoc/latest/bundles/redoc.standalone.js"></script>
  <script>
    Redoc.init('/openapi.json', {
      scrollYOffset: 0,
      hideDownloadButton: false,
      expandResponses: "200"
    }, document.getElementById('redoc-container'));
  </script>
</body>
</html>"###;

pub fn swagger_ui_html() -> &'static str {
    SWAGGER_UI_HTML
}

pub fn redoc_html() -> &'static str {
    REDOC_HTML
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_document() {
        let doc = OpenApiBuilder::new("Test API", "1.0.0").build();
        let json = serde_json::to_string_pretty(&doc).unwrap();
        assert!(json.contains("3.1.0"));
        assert!(json.contains("Test API"));
    }

    #[test]
    fn test_with_server_and_tags() {
        let doc = OpenApiBuilder::new("My API", "0.1.0")
            .server("https://api.example.com", Some("Production"))
            .tag("users", Some("User operations"))
            .build();
        assert!(doc.servers.is_some());
        assert_eq!(doc.servers.as_ref().unwrap().len(), 1);
        assert!(doc.tags.is_some());
    }

    #[test]
    fn test_get_operation() {
        let doc = OpenApiBuilder::new("Test", "1.0")
            .operation(
                Method::GET,
                "/users/{id}",
                Operation {
                    summary: Some("Get user by ID".to_string()),
                    description: None,
                    operation_id: Some("get_users_id".to_string()),
                    tags: vec!["users".to_string()],
                    parameters: vec![Parameter {
                        name: "id".to_string(),
                        location: ParameterLocation::Path,
                        description: Some("User ID".to_string()),
                        required: true,
                        schema: Some(Schema::integer()),
                    }],
                    request_body: None,
                    responses: {
                        let mut m = BTreeMap::new();
                        m.insert(
                            "200".to_string(),
                            Response { description: "User found".to_string(), content: None },
                        );
                        m
                    },
                    deprecated: None,
                    extensions: Default::default(),
                },
            )
            .build();

        let json = serde_json::to_value(&doc).unwrap();
        let path = json.pointer("/paths/~1users~1{id}").unwrap();
        let get = path.pointer("/get").unwrap();
        assert_eq!(get["summary"], "Get user by ID");
        assert_eq!(get["parameters"][0]["name"], "id");
        assert_eq!(get["parameters"][0]["in"], "path");
    }

    #[test]
    fn test_post_operation_with_body() {
        let body_schema = serde_json::json!({
            "name": "string",
            "email": "string"
        });
        let response_schema = serde_json::json!({
            "id": 0,
            "name": "string"
        });

        let mut registry = OpenApiRegistry::new();
        registry.register(RouteMeta {
            method: Method::POST,
            path: "/users".to_string(),
            summary: Some("Create user".to_string()),
            description: None,
            tags: vec!["users".to_string()],
            request_body_schema: Some(body_schema),
            response_schema: Some(response_schema),
            deprecated: false,
            experimental: false,
            status_code: None,
            responses: None,
            operation_id: None,
            openapi_extra: None,
            include_in_schema: true,
        });

        let doc = registry.generate("Test", "1.0.0");
        let json = serde_json::to_value(&doc).unwrap();
        let path = json.pointer("/paths/~1users").unwrap();
        let post = path.pointer("/post").unwrap();
        assert!(post.get("requestBody").is_some());
        // parameters is skipped when empty (Vec::is_empty → skip_serializing_if)
        assert!(
            post.get("parameters").is_none() || post["parameters"].as_array().unwrap().is_empty()
        );
    }

    #[test]
    fn test_path_parameters_extracted() {
        let mut registry = OpenApiRegistry::new();
        registry.register(RouteMeta {
            method: Method::GET,
            path: "/users/{id}/posts/{postId}".to_string(),
            summary: Some("Get post by user".to_string()),
            description: None,
            tags: vec![],
            request_body_schema: None,
            response_schema: None,
            deprecated: false,
            experimental: false,
            status_code: None,
            responses: None,
            operation_id: None,
            openapi_extra: None,
            include_in_schema: true,
        });

        let doc = registry.generate("Test", "1.0.0");
        let json = serde_json::to_value(&doc).unwrap();
        let params = json.pointer("/paths/~1users~1{id}~1posts~1{postId}/get/parameters").unwrap();
        assert_eq!(params.as_array().unwrap().len(), 2);
        assert_eq!(params[0]["name"], "id");
        assert_eq!(params[1]["name"], "postId");
    }

    #[test]
    fn test_query_field_serialized() {
        let doc = OpenApiBuilder::new("Q", "1.0.0")
            .operation(
                crate::query_method(),
                "/search",
                Operation {
                    summary: Some("Search".to_string()),
                    description: None,
                    operation_id: Some("query_search".to_string()),
                    tags: vec![],
                    parameters: vec![],
                    request_body: Some(RequestBody {
                        description: None,
                        content: {
                            let mut c = std::collections::BTreeMap::new();
                            c.insert(
                                "application/x-www-form-urlencoded".to_string(),
                                MediaType { schema: Some(Schema::string()) },
                            );
                            c
                        },
                        required: true,
                    }),
                    responses: {
                        let mut m = std::collections::BTreeMap::new();
                        m.insert(
                            "200".to_string(),
                            Response { description: "ok".to_string(), content: None },
                        );
                        m
                    },
                    deprecated: None,
                    extensions: Default::default(),
                },
            )
            .build();
        let json = serde_json::to_value(&doc).unwrap();
        let op = json.pointer("/paths/~1search/query").expect("query op present");
        assert_eq!(op["summary"], "Search");
        assert!(op.get("requestBody").is_some());
    }

    #[test]
    fn test_query_tagged_experimental() {
        let mut registry = OpenApiRegistry::new();
        registry.register(RouteMeta {
            method: crate::query_method(),
            path: "/search".to_string(),
            summary: Some("Search".to_string()),
            description: None,
            tags: vec![],
            request_body_schema: Some(serde_json::json!({"q": "string"})),
            response_schema: None,
            deprecated: false,
            experimental: true,
            status_code: None,
            responses: None,
            operation_id: None,
            openapi_extra: None,
            include_in_schema: true,
        });

        let doc = registry.generate("Test", "1.0.0");
        let json = serde_json::to_value(&doc).unwrap();
        let op = json.pointer("/paths/~1search/query").expect("query op present");
        let tags = op["tags"].as_array().expect("tags array");
        assert!(tags.iter().any(|t| t == "experimental"));
        // QUERY carries a request body per RFC 10008.
        assert!(op.get("requestBody").is_some());
    }

    #[test]
    fn test_query_not_tagged_when_not_experimental() {
        let mut registry = OpenApiRegistry::new();
        registry.register(RouteMeta {
            method: crate::query_method(),
            path: "/search".to_string(),
            summary: Some("Search".to_string()),
            description: None,
            tags: vec!["search".to_string()],
            request_body_schema: None,
            response_schema: None,
            deprecated: false,
            experimental: false,
            status_code: None,
            responses: None,
            operation_id: None,
            openapi_extra: None,
            include_in_schema: true,
        });

        let doc = registry.generate("Test", "1.0.0");
        let json = serde_json::to_value(&doc).unwrap();
        let op = json.pointer("/paths/~1search/query").expect("query op present");
        let tags = op["tags"].as_array().expect("tags array");
        assert!(tags.iter().all(|t| t != "experimental"));
    }

    #[test]
    fn test_schema_object_builder() {
        let schema = SchemaObject::object()
            .with_property("name", Schema::string())
            .with_property("age", Schema::integer())
            .with_required("name");

        let json = serde_json::to_value(&schema).unwrap();
        assert_eq!(json["type"], "object");
        assert!(json["properties"]["name"]["type"].as_str().is_some());
        assert_eq!(json["required"][0], "name");
    }

    #[test]
    fn test_json_value_to_schema() {
        let v =
            serde_json::json!({"name": "Alice", "age": 30, "active": true, "scores": [1, 2, 3]});
        let schema = json_value_to_schema(&v);
        let json = serde_json::to_value(&schema).unwrap();
        if let Some(obj) = json.as_object() {
            assert_eq!(
                obj.get("properties").and_then(|p| p.get("name")).and_then(|n| n.get("type")),
                Some(&serde_json::json!("string"))
            );
        }
    }

    #[test]
    fn test_generated_json_is_valid_openapi() {
        let doc = OpenApiBuilder::new("Valid API", "2.0.0")
            .server("http://localhost:8080", None)
            .operation(
                Method::GET,
                "/health",
                Operation {
                    summary: Some("Health check".to_string()),
                    description: None,
                    operation_id: Some("get_health".to_string()),
                    tags: vec![],
                    parameters: vec![],
                    request_body: None,
                    responses: {
                        let mut m = BTreeMap::new();
                        m.insert(
                            "200".to_string(),
                            Response { description: "OK".to_string(), content: None },
                        );
                        m
                    },
                    deprecated: None,
                    extensions: Default::default(),
                },
            )
            .build();

        let json = serde_json::to_value(&doc).unwrap();
        assert_eq!(json["openapi"], "3.1.0");
        assert_eq!(json["info"]["title"], "Valid API");
        assert_eq!(json["info"]["version"], "2.0.0");
        assert!(json.get("paths").is_some());
        assert!(json.get("servers").is_some());
    }
}
