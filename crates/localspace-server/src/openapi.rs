//! `/api/v1/openapi.json`, generated from `proto` (v2 §5). The schemas are
//! the Rust types' JSON Schemas; the paths are the routes in `lib.rs`. There
//! is no second description of the API to drift from the first.

use localspace_proto as proto;
use schemars::generate::SchemaSettings;
use serde_json::{Value, json};
use std::sync::OnceLock;

pub fn document() -> &'static Value {
    static DOC: OnceLock<Value> = OnceLock::new();
    DOC.get_or_init(build)
}

fn build() -> Value {
    let mut generator = SchemaSettings::openapi3().into_generator();
    let envelope = generator.subschema_for::<proto::Envelope>();
    let request = generator.subschema_for::<proto::Request>();
    let response = generator.subschema_for::<proto::Response>();
    let event = generator.subschema_for::<proto::Event>();
    let environment = generator.subschema_for::<proto::EnvironmentState>();
    let definitions = generator.take_definitions(true);

    let response_of = |schema: &schemars::Schema, description: &str| {
        json!({
            "description": description,
            "content": {"application/json": {"schema": schema}}
        })
    };
    let unauthorized = json!({"description": "no valid session cookie or bearer token"});

    let mut paths = serde_json::Map::new();
    paths.insert(
        "/api/v1/login".into(),
        json!({"post": {
            "summary": "Present the server's token; sets the session cookie",
            "requestBody": {"content": {"application/json": {"schema": {
                "type": "object", "required": ["token"],
                "properties": {"token": {"type": "string"}}
            }}}},
            "responses": {"200": {"description": "signed in"}, "401": unauthorized}
        }}),
    );
    paths.insert(
        "/api/v1/logout".into(),
        json!({"post": {"summary": "Clear the session cookie", "responses": {"200": {"description": "signed out"}}}}),
    );
    paths.insert(
        "/api/v1/me".into(),
        json!({"get": {"summary": "Who this session is, and the server's mode", "responses": {"200": {"description": "ok"}, "401": unauthorized}}}),
    );
    paths.insert(
        "/api/v1/request".into(),
        json!({"post": {
            "summary": "Any request; its response",
            "requestBody": {"required": true, "content": {"application/json": {"schema": request}}},
            "responses": {
                "200": response_of(&response, "the response"),
                "401": unauthorized,
                "504": {"description": "Core did not answer in time"}
            }
        }}),
    );
    for (path, summary) in [
        ("/api/v1/environment", "GetEnvironment"),
        ("/api/v1/catalog", "ListCatalog"),
        ("/api/v1/history", "GetHistory"),
        ("/api/v1/task", "GetTask"),
        ("/api/v1/active-set", "GetActiveSet"),
        ("/api/v1/lock", "GetLock"),
        ("/api/v1/documents", "ListDocuments"),
    ] {
        paths.insert(
            path.into(),
            json!({"get": {
                "summary": format!("The response to {summary}"),
                "responses": {"200": response_of(&response, "the response"), "401": unauthorized}
            }}),
        );
    }
    paths.insert(
        "/api/v1/docs/{harness}".into(),
        json!({"get": {
            "summary": "The response to GetDocJson for one harness",
            "parameters": [{"name": "harness", "in": "path", "required": true, "schema": {"type": "string"}}],
            "responses": {"200": response_of(&response, "the response"), "401": unauthorized}
        }}),
    );
    paths.insert(
        "/api/v1/documents/{id}/content".into(),
        json!({"get": {
            "summary": "A file document's bytes at its head (GetDocBlob), as an attachment with nosniff",
            "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
            "responses": {
                "200": {"description": "the file, Content-Type its media type, Content-Disposition attachment"},
                "401": unauthorized,
                "404": {"description": "not a file the caller may read, or one with no content"}
            }
        }}),
    );
    paths.insert(
        "/api/v1/artifacts".into(),
        json!({"post": {
            "summary": "ProduceArtifact with the file as the body: Content-Type is the kind's media type; the rest is in the query",
            "parameters": [
                {"name": "harness", "in": "query", "required": true, "schema": {"type": "string"}},
                {"name": "view", "in": "query", "required": true, "schema": {"type": "string"}},
                {"name": "kind", "in": "query", "required": true, "schema": {"type": "string"}, "description": "an interchange type, e.g. image.v1"},
                {"name": "name", "in": "query", "required": true, "schema": {"type": "string"}},
                {"name": "fields", "in": "query", "required": false, "schema": {"type": "string"}, "description": "a JSON object with the fields the kind requires"},
                {"name": "summary", "in": "query", "required": false, "schema": {"type": "string"}}
            ],
            "requestBody": {"required": true, "content": {"*/*": {"schema": {"type": "string", "format": "binary"}}}},
            "responses": {
                "200": response_of(&response, "the artifact, or an error"),
                "400": {"description": "no Content-Type, or fields that are not a JSON object"},
                "401": unauthorized,
                "413": {"description": "over the 200 MiB limit"}
            }
        }}),
    );
    paths.insert(
        "/ws/json".into(),
        json!({"get": {
            "summary": "WebSocket: Envelope as JSON text frames — events out, requests in, responses back under their id",
            "responses": {"101": {"description": "switching protocols"}, "401": unauthorized}
        }}),
    );
    paths.insert(
        "/ws".into(),
        json!({"get": {"summary": "WebSocket: Envelope as postcard binary frames, for the egui Client", "responses": {"101": {"description": "switching protocols"}}}}),
    );
    for (path, summary) in [
        ("/healthz", "process up"),
        ("/readyz", "a Core answers"),
        ("/metrics", "Prometheus metrics"),
    ] {
        paths.insert(
            path.into(),
            json!({"get": {"summary": summary, "responses": {"200": {"description": "ok"}}}}),
        );
    }

    let mut schemas = definitions;
    schemas.insert("Envelope".into(), envelope.to_value());
    schemas.insert("EnvironmentState".into(), environment.to_value());
    schemas.insert("Event".into(), event.to_value());

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "localSpace",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Generated from localspace-proto. Request, Response and Event are the whole contract; \
                            the TypeScript client's types are generated from the same source."
        },
        "paths": Value::Object(paths),
        "components": {
            "schemas": Value::Object(schemas),
            "securitySchemes": {
                "cookie": {"type": "apiKey", "in": "cookie", "name": crate::auth::COOKIE},
                "bearer": {"type": "http", "scheme": "bearer"}
            }
        },
        "security": [{"cookie": []}, {"bearer": []}]
    })
}
