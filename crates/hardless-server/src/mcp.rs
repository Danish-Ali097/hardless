//! `/mcp`: the MCP server for agents in containers or on other machines (FR-43), as Streamable
//! HTTP. Its tools call this server's REST routes in-process, so they behave exactly like the
//! REST API and `hardless mcp --url` (PRD section 11.4).

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request};
use axum::routing::get;
use axum::{middleware, Router};
use hardless_mcp::{BoxFuture, Method, Rest, RestResponse};
use serde_json::Value;
use tower::ServiceExt;

use crate::{api, security, AppState};

/// Largest REST answer the MCP tools read (rendered PNGs are the big ones).
const MAX_BODY: usize = 64 * 1024 * 1024;

/// The REST API, called in-process. The `/mcp` route itself checks the token, so these calls
/// skip it.
#[derive(Clone)]
struct InProcess {
    routes: Router,
}

impl Rest for InProcess {
    fn send<'a>(
        &'a self,
        method: Method,
        path: &'a str,
        body: Option<Value>,
    ) -> BoxFuture<'a, Result<RestResponse, String>> {
        Box::pin(async move {
            let mut request = Request::builder().method(method.as_str()).uri(path);
            let body = match body {
                Some(value) => {
                    request = request.header(header::CONTENT_TYPE, "application/json");
                    Body::from(value.to_string())
                }
                None => Body::empty(),
            };
            let request = request.body(body).map_err(|e| e.to_string())?;
            let response = self
                .routes
                .clone()
                .oneshot(request)
                .await
                .map_err(|e| e.to_string())?;
            let status = response.status().as_u16();
            let body = to_bytes(response.into_body(), MAX_BODY)
                .await
                .map_err(|e| e.to_string())?;
            Ok(RestResponse {
                status,
                body: body.to_vec(),
            })
        })
    }
}

/// The `/mcp` route, behind the token check (NFR-02). On localhost (the default, NFR-01) it
/// answers only loopback `Host` names, which blocks DNS rebinding from web pages. Bound to the
/// network (`--host 0.0.0.0`, Docker) it answers any `Host`, as agents in other containers use
/// names like `hardless:8080`; the server is reachable anyway and the token protects it.
pub fn routes(state: AppState) -> Router<AppState> {
    let rest = InProcess {
        routes: api::routes()
            .route("/info", get(crate::info))
            .with_state(state.clone()),
    };
    let hosts = state.loopback_only.then(|| {
        ["localhost", "127.0.0.1", "::1"]
            .iter()
            .map(|h| (*h).to_owned())
            .collect()
    });
    let service = hardless_mcp::http_service(Arc::new(rest), false, hosts);
    Router::new()
        .route_service("/mcp", service)
        .layer(middleware::from_fn_with_state(
            state,
            security::require_token,
        ))
}
