use std::collections::BTreeMap;
use std::path::Path;

use justapi_core::openapi::{OpenApiDocument, Operation, ParameterLocation};

/// Supported target languages for client generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientLanguage {
    Python,
    Typescript,
}

impl std::str::FromStr for ClientLanguage {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "py" | "python" => Ok(ClientLanguage::Python),
            "ts" | "typescript" | "js" => Ok(ClientLanguage::Typescript),
            other => anyhow::bail!(
                "unsupported language '{}' (expected python or typescript)",
                other
            ),
        }
    }
}

impl ClientLanguage {
    pub fn extension(&self) -> &'static str {
        match self {
            ClientLanguage::Python => "py",
            ClientLanguage::Typescript => "ts",
        }
    }

    pub fn module_name(&self) -> &'static str {
        match self {
            ClientLanguage::Python => "client",
            ClientLanguage::Typescript => "client",
        }
    }
}

/// A single parameter with its original spec name, a sanitized argument
/// name safe for the target language, and its scalar schema type (if known).
struct Param {
    original: String,
    arg: String,
    ty: Option<String>,
}

/// One operation extracted from the spec, ready to emit.
struct Op {
    method: String,
    raw_path: String,
    name: String,
    summary: Option<String>,
    path_params: Vec<Param>,
    query_params: Vec<Param>,
    header_params: Vec<Param>,
    has_body: bool,
}

/// Build a valid identifier from an arbitrary operation id / hint.
fn sanitize_ident(s: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            out.push(if upper { ch.to_ascii_uppercase() } else { ch });
            upper = false;
        } else {
            upper = true;
        }
    }
    // Collapse leading/trailing digits and ensure non-empty.
    let out: String = out.trim_matches(|c: char| !c.is_alphabetic()).to_string();
    if out.is_empty() {
        "call".to_string()
    } else {
        out
    }
}

fn python_type(schema_type: Option<&str>) -> &'static str {
    match schema_type {
        Some("string") => "str",
        Some("integer") => "int",
        Some("number") => "float",
        Some("boolean") => "bool",
        Some("array") => "list",
        Some("object") => "dict",
        _ => "Any",
    }
}

fn ts_type(schema_type: Option<&str>) -> &'static str {
    match schema_type {
        Some("string") => "string",
        Some("integer") | Some("number") => "number",
        Some("boolean") => "boolean",
        Some("array") => "unknown[]",
        Some("object") => "Record<string, unknown>",
        _ => "unknown",
    }
}

fn collect_ops(doc: &OpenApiDocument) -> Vec<Op> {
    let mut ops: Vec<Op> = Vec::new();
    let methods = [
        ("get", &doc.paths),
        ("post", &doc.paths),
        ("put", &doc.paths),
        ("delete", &doc.paths),
        ("patch", &doc.paths),
        ("head", &doc.paths),
        ("options", &doc.paths),
        ("query", &doc.paths),
    ];

    for (method, paths) in methods {
        for (raw_path, item) in paths {
            let op: Option<&Operation> = match method {
                "get" => item.get.as_ref(),
                "post" => item.post.as_ref(),
                "put" => item.put.as_ref(),
                "delete" => item.delete.as_ref(),
                "patch" => item.patch.as_ref(),
                "head" => item.head.as_ref(),
                "options" => item.options.as_ref(),
                "query" => item.query.as_ref(),
                _ => None,
            };
            let Some(op) = op else { continue };

            let mut path_params = Vec::new();
            let mut query_params = Vec::new();
            let mut header_params = Vec::new();
            for p in &op.parameters {
                let t = p.schema.as_ref().and_then(|s| match s {
                    justapi_core::openapi::Schema::Object(o) => o.schema_type.clone(),
                    _ => None,
                });
                let arg = sanitize_ident(&p.name);
                let param = Param {
                    original: p.name.clone(),
                    arg,
                    ty: t,
                };
                match p.location {
                    ParameterLocation::Path => path_params.push(param),
                    ParameterLocation::Query => query_params.push(param),
                    ParameterLocation::Header => header_params.push(param),
                    ParameterLocation::Cookie => {}
                }
            }

            let name = op
                .operation_id
                .clone()
                .map(|id| sanitize_ident(&id))
                .unwrap_or_else(|| {
                    let base = raw_path
                        .split('/')
                        .filter(|s| !s.is_empty())
                        .map(sanitize_ident)
                        .collect::<Vec<_>>()
                        .join("_");
                    format!("{}_{}", method, base)
                });

            let has_body = op.request_body.is_some();

            ops.push(Op {
                method: method.to_ascii_uppercase().to_string(),
                raw_path: raw_path.clone(),
                name,
                summary: op.summary.clone(),
                path_params,
                query_params,
                header_params,
                has_body,
            });
        }
    }

    // De-duplicate method names (case-insensitive), appending an index on clash.
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for op in ops.iter_mut() {
        let key = op.name.to_ascii_lowercase();
        let count = seen.entry(key).or_insert(0);
        if *count > 0 {
            op.name = format!("{}_{}", op.name, *count);
        }
        *count += 1;
    }

    ops
}

fn base_url(doc: &OpenApiDocument) -> String {
    doc.servers
        .as_ref()
        .and_then(|s| s.first())
        .map(|s| s.url.clone())
        .unwrap_or_else(|| "http://localhost:8080".to_string())
}

pub fn generate_client(doc: &OpenApiDocument, lang: &ClientLanguage) -> String {
    let ops = collect_ops(doc);
    let base = base_url(doc);
    match lang {
        ClientLanguage::Python => generate_python(&ops, &base),
        ClientLanguage::Typescript => generate_typescript(&ops, &base),
    }
}

fn generate_python(ops: &[Op], base: &str) -> String {
    let mut methods = String::new();
    for op in ops {
        methods.push_str(&python_method(op));
    }

    format!(
        r#""""{}
Auto-generated by `justapi gen client`. Do not edit by hand.

Base URL: {}
"""
from __future__ import annotations

from typing import Any

import requests


class Client:
    def __init__(
        self,
        base_url: str = {base:?},
        *,
        timeout: float = 30.0,
        headers: dict[str, str] | None = None,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self.session = requests.Session()
        if headers:
            self.session.headers.update(headers)

    def _request(
        self,
        method: str,
        path: str,
        *,
        params: dict[str, Any] | None = None,
        json: Any | None = None,
        data: Any | None = None,
        headers: dict[str, str] | None = None,
    ) -> Any:
        url = self.base_url + path
        resp = self.session.request(
            method,
            url,
            params=params,
            json=json,
            data=data,
            headers=headers,
            timeout=self.timeout,
        )
        resp.raise_for_status()
        ctype = resp.headers.get("content-type", "")
        if resp.content and ctype.startswith("application/json"):
            return resp.json()
        return resp

{methods}

if __name__ == "__main__":
    client = Client()
"#,
        "JustAPI generated Python client.", base
    )
}

fn python_method(op: &Op) -> String {
    let path = &op.raw_path;
    let mut args: Vec<String> = Vec::new();
    let mut doc: Vec<String> = Vec::new();

    for p in &op.path_params {
        let ty = python_type(p.ty.as_deref());
        args.push(format!("{}: {}", p.arg, ty));
    }
    for p in &op.query_params {
        let ty = python_type(p.ty.as_deref());
        args.push(format!("{}: {} | None = None", p.arg, ty));
    }
    for p in &op.header_params {
        args.push(format!("{}: str | None = None", p.arg));
    }
    if op.has_body {
        args.push("body: Any | None = None".to_string());
    }
    args.push("extra_headers: dict[str, str] | None = None".to_string());

    let signature = format!("    def {}(self, {}) -> Any:", op.name, args.join(", "));

    if let Some(summary) = &op.summary {
        doc.push(format!("        \"\"\"{}\"\"\"", summary));
    } else {
        doc.push("        \"\"\"Auto-generated endpoint call.\"\"\"".to_string());
    }

    // Path substitution.
    let path_stmt = if op.path_params.is_empty() {
        format!("        path = {}", path.quoted())
    } else {
        let mut fmt_path = path.clone();
        let mut fmt_args: Vec<String> = Vec::new();
        for p in &op.path_params {
            fmt_path = fmt_path.replace(&format!("{{{}}}", p.original), &format!("{{{}}}", p.arg));
            fmt_args.push(format!("{}={}", p.arg, p.arg));
        }
        format!(
            "        path = {}.format({})",
            fmt_path.quoted(),
            fmt_args.join(", ")
        )
    };

    let mut body_lines: Vec<String> = Vec::new();
    if !op.query_params.is_empty() {
        let keys: Vec<String> = op
            .query_params
            .iter()
            .map(|p| format!("{:?}: {}", p.original, p.arg))
            .collect();
        body_lines.push(format!(
            "        params = {{k: v for k, v in {{{}}}.items() if v is not None}}",
            keys.join(", ")
        ));
    }
    let mut call_args: Vec<String> = Vec::new();
    if !op.query_params.is_empty() {
        call_args.push("params=params".to_string());
    }
    if op.has_body {
        call_args.push("json=body".to_string());
    }
    if !op.header_params.is_empty() {
        let mut parts: Vec<String> = op
            .header_params
            .iter()
            .map(|p| {
                format!(
                    "            {:?}: {} if {} is not None else None,",
                    p.original, p.arg, p.arg
                )
            })
            .collect();
        parts.push("            **(extra_headers or {})".to_string());
        body_lines.push(format!(
            "        headers = {{\n{}\n        }}",
            parts.join("\n")
        ));
    } else {
        call_args.push("headers=extra_headers".to_string());
    }

    let call = format!(
        "        result = self._request({}, path{})",
        op.method.quoted(),
        if call_args.is_empty() {
            String::new()
        } else {
            format!(", {}", call_args.join(", "))
        }
    );

    let body = format!(
        "{}\n{}\n{}\n{}\n        return result\n\n",
        doc.join("\n"),
        path_stmt,
        body_lines.join("\n"),
        call,
    );

    format!("{}\n{}\n", signature, body)
}

trait Quote {
    fn quoted(&self) -> String;
}

impl Quote for str {
    fn quoted(&self) -> String {
        format!("{:?}", self)
    }
}

fn generate_typescript(ops: &[Op], base: &str) -> String {
    let mut methods = String::new();
    for op in ops {
        methods.push_str(&ts_method(op));
    }

    format!(
        r#"// JustAPI generated TypeScript client.
// Auto-generated by `justapi gen client`. Do not edit by hand.
// Base URL: {base}

export interface ClientOptions {{
  baseUrl?: string;
  headers?: Record<string, string>;
  fetch?: typeof fetch;
}}

export class Client {{
  private baseUrl: string;
  private headers: Record<string, string>;
  private fetcher: typeof fetch;

  constructor(opts: ClientOptions = {{}}) {{
    this.baseUrl = (opts.baseUrl ?? "{base}").replace(/\/$/, "");
    this.headers = opts.headers ?? {{}};
    this.fetcher = opts.fetch ?? fetch;
  }}

  private async request<T = unknown>(
    method: string,
    path: string,
    opts: {{ params?: Record<string, unknown>; body?: unknown; headers?: Record<string, string> }} = {{}},
  ): Promise<T> {{
    const url = new URL(this.baseUrl + path);
    if (opts.params) {{
      for (const [k, v] of Object.entries(opts.params)) {{
        if (v !== undefined && v !== null) url.searchParams.set(k, String(v));
      }}
    }}
    const resp = await this.fetcher(url.toString(), {{
      method,
      headers: {{ "content-type": "application/json", ...this.headers, ...(opts.headers ?? {{}}) }},
      body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
    }});
    if (!resp.ok) throw new Error(`Request failed: ${{resp.status}} ${{resp.statusText}}`);
    const ctype = resp.headers.get("content-type") ?? "";
    if (ctype.includes("application/json")) return (await resp.json()) as T;
    return (await resp.text()) as unknown as T;
  }}

{methods}
}}
"#
    )
}

fn ts_method(op: &Op) -> String {
    let mut args: Vec<String> = Vec::new();
    for p in &op.path_params {
        let ty = ts_type(p.ty.as_deref());
        args.push(format!("{}: {}", p.arg, ty));
    }
    for p in &op.query_params {
        let ty = ts_type(p.ty.as_deref());
        args.push(format!("{}?: {}", p.arg, ty));
    }
    for p in &op.header_params {
        args.push(format!("{}?: string", p.arg));
    }
    if op.has_body {
        args.push("body?: unknown".to_string());
    }
    args.push("extraHeaders?: Record<string, string>".to_string());

    let mut body_lines: Vec<String> = Vec::new();

    let path_stmt = if op.path_params.is_empty() {
        format!("    const path = {};", op.raw_path.quoted())
    } else {
        let repl: Vec<String> = op
            .path_params
            .iter()
            .map(|p| {
                format!(
                    "path = path.replace(`{{{}}}`, String({}));",
                    p.original, p.arg
                )
            })
            .collect();
        format!(
            "    let path = {};\n    {}",
            op.raw_path.quoted(),
            repl.join("\n    ")
        )
    };
    body_lines.push(path_stmt);

    if !op.query_params.is_empty() {
        let obj = op
            .query_params
            .iter()
            .map(|p| format!("      {:?}: {},", p.original, p.arg))
            .collect::<Vec<_>>()
            .join("\n");
        body_lines.push(format!("    const params = {{\n{}\n    }};", obj));
    }
    if !op.header_params.is_empty() {
        let obj = op
            .header_params
            .iter()
            .map(|p| format!("      {:?}: {},", p.original, p.arg))
            .collect::<Vec<_>>()
            .join("\n");
        body_lines.push(format!(
            "    const headers = {{\n{}\n      ...(extraHeaders ?? {{}}),\n    }};",
            obj
        ));
    }

    let mut call_args: Vec<String> = Vec::new();
    if !op.query_params.is_empty() {
        call_args.push("params".to_string());
    }
    if op.has_body {
        call_args.push("body".to_string());
    }
    if !op.header_params.is_empty() {
        call_args.push("headers".to_string());
    } else {
        call_args.push("extraHeaders".to_string());
    }

    let summary = op
        .summary
        .clone()
        .map(|s| format!("  /** {} */\n", s))
        .unwrap_or_default();

    let call = format!(
        "    return this.request(\"{}\", path{}) as Promise<unknown>;",
        op.method,
        if call_args.is_empty() {
            String::new()
        } else {
            format!(
                ", {{ {} }}",
                call_args
                    .iter()
                    .map(|a| format!("{a}: {}", a))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    );

    format!(
        "\n{}  {}({}): Promise<unknown> {{\n{}\n{}\n  }}\n",
        summary,
        op.name,
        args.join(", "),
        body_lines.join("\n"),
        call,
    )
}

/// Public entry point used by the CLI.
pub fn write_client(
    doc: &OpenApiDocument,
    lang: &ClientLanguage,
    output_dir: &Path,
) -> anyhow::Result<std::path::PathBuf> {
    std::fs::create_dir_all(output_dir)?;
    let code = generate_client(doc, lang);
    let file_name = format!("{}.{}", lang.module_name(), lang.extension());
    let out_path = output_dir.join(file_name);
    std::fs::write(&out_path, code)?;
    Ok(out_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::Method;
    use justapi_core::openapi::{
        OpenApiBuilder, Operation, Parameter, ParameterLocation, RequestBody, Schema,
    };

    fn sample_doc() -> OpenApiDocument {
        let body_schema = Schema::Object(justapi_core::openapi::SchemaObject::object());
        OpenApiBuilder::new("Demo API", "1.0.0")
            .server("https://api.example.com", Some("Prod"))
            .operation(
                Method::GET,
                "/users",
                Operation {
                    summary: Some("List users".to_string()),
                    description: None,
                    operation_id: Some("listUsers".to_string()),
                    tags: vec![],
                    parameters: vec![
                        Parameter {
                            name: "limit".to_string(),
                            location: ParameterLocation::Query,
                            description: None,
                            required: false,
                            schema: Some(Schema::integer()),
                        },
                        Parameter {
                            name: "X-Trace".to_string(),
                            location: ParameterLocation::Header,
                            description: None,
                            required: false,
                            schema: Some(Schema::string()),
                        },
                    ],
                    request_body: None,
                    responses: {
                        let mut m = std::collections::BTreeMap::new();
                        m.insert(
                            "200".to_string(),
                            justapi_core::openapi::Response {
                                description: "ok".to_string(),
                                content: None,
                            },
                        );
                        m
                    },
                    deprecated: None,
                },
            )
            .operation(
                Method::GET,
                "/users/{id}",
                Operation {
                    summary: Some("Get user".to_string()),
                    description: None,
                    operation_id: Some("getUser".to_string()),
                    tags: vec![],
                    parameters: vec![Parameter {
                        name: "id".to_string(),
                        location: ParameterLocation::Path,
                        description: None,
                        required: true,
                        schema: Some(Schema::integer()),
                    }],
                    request_body: None,
                    responses: {
                        let mut m = std::collections::BTreeMap::new();
                        m.insert(
                            "200".to_string(),
                            justapi_core::openapi::Response {
                                description: "ok".to_string(),
                                content: None,
                            },
                        );
                        m
                    },
                    deprecated: None,
                },
            )
            .operation(
                Method::POST,
                "/users",
                Operation {
                    summary: Some("Create user".to_string()),
                    description: None,
                    operation_id: Some("createUser".to_string()),
                    tags: vec![],
                    parameters: vec![],
                    request_body: Some(RequestBody {
                        description: None,
                        content: {
                            let mut c = std::collections::BTreeMap::new();
                            c.insert(
                                "application/json".to_string(),
                                justapi_core::openapi::MediaType {
                                    schema: Some(body_schema),
                                },
                            );
                            c
                        },
                        required: true,
                    }),
                    responses: {
                        let mut m = std::collections::BTreeMap::new();
                        m.insert(
                            "200".to_string(),
                            justapi_core::openapi::Response {
                                description: "ok".to_string(),
                                content: None,
                            },
                        );
                        m
                    },
                    deprecated: None,
                },
            )
            .build()
    }

    #[test]
    fn test_sanitize_ident() {
        assert_eq!(sanitize_ident("list_users"), "listUsers");
        assert_eq!(sanitize_ident("getUser"), "getUser");
        assert_eq!(sanitize_ident("foo-bar"), "fooBar");
        assert_eq!(sanitize_ident("foo bar.baz"), "fooBarBaz");
        assert_eq!(sanitize_ident("123"), "call");
        assert_eq!(sanitize_ident("X-Trace"), "XTrace");
    }

    #[test]
    fn test_python_generation() {
        let doc = sample_doc();
        let py = generate_client(&doc, &ClientLanguage::Python);
        assert!(py.contains("class Client:"));
        assert!(py.contains("def listUsers(self, limit: int | None = None"));
        assert!(py.contains("def getUser(self, id: int"));
        assert!(py.contains("def createUser(self, body: Any | None = None"));
        // Header param sanitized, original name preserved in dict key.
        assert!(py.contains("XTrace: str | None = None"));
        assert!(py.contains("\"X-Trace\": XTrace"));
        // Path substitution uses sanitized arg name.
        assert!(py.contains("path = \"/users/{id}\".format(id=id)"));
    }

    #[test]
    fn test_typescript_generation() {
        let doc = sample_doc();
        let ts = generate_client(&doc, &ClientLanguage::Typescript);
        assert!(ts.contains("export class Client"));
        assert!(ts.contains("getUser(id: number"));
        assert!(ts.contains("path.replace(`{id}`, String(id))"));
        assert!(ts.contains("XTrace?: string"));
        assert!(ts.contains("\"X-Trace\": XTrace"));
    }

    #[test]
    fn test_language_parsing() {
        assert_eq!(
            "python".parse::<ClientLanguage>().unwrap(),
            ClientLanguage::Python
        );
        assert_eq!(
            "ts".parse::<ClientLanguage>().unwrap(),
            ClientLanguage::Typescript
        );
        assert!("cobol".parse::<ClientLanguage>().is_err());
    }

    #[test]
    fn test_roundtrip_deserialize() {
        let json = serde_json::to_string(&sample_doc()).unwrap();
        let back: OpenApiDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(back.info.title, "Demo API");
        assert!(back.paths.get("/users").unwrap().get.is_some());
    }

    #[test]
    fn test_query_operation_generated() {
        use justapi_core::openapi::{MediaType, Operation, RequestBody};
        let doc = OpenApiBuilder::new("Q", "1.0.0")
            .operation(
                justapi_core::query_method(),
                "/search",
                Operation {
                    summary: Some("Search".to_string()),
                    description: None,
                    operation_id: Some("searchItems".to_string()),
                    tags: vec![],
                    parameters: vec![],
                    request_body: Some(RequestBody {
                        description: None,
                        content: {
                            let mut c = std::collections::BTreeMap::new();
                            c.insert(
                                "application/x-www-form-urlencoded".to_string(),
                                MediaType {
                                    schema: Some(Schema::string()),
                                },
                            );
                            c
                        },
                        required: true,
                    }),
                    responses: {
                        let mut m = std::collections::BTreeMap::new();
                        m.insert(
                            "200".to_string(),
                            justapi_core::openapi::Response {
                                description: "ok".to_string(),
                                content: None,
                            },
                        );
                        m
                    },
                    deprecated: None,
                },
            )
            .build();
        let py = generate_client(&doc, &ClientLanguage::Python);
        assert!(py.contains("def searchItems(self, body: Any | None = None"));
        let ts = generate_client(&doc, &ClientLanguage::Typescript);
        assert!(ts.contains("searchItems(body?: unknown"));
    }
}
