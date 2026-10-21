//! How the MCP server reaches Hardless: the REST API (PRD section 11.1), over the network for
//! `hardless mcp --url` or in-process for `/mcp` on the server. Every tool is a REST call, so
//! agents can do exactly what the dashboard can (PRD section 11.4).

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

/// A boxed future, so [`Rest`] can be a trait object.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// An HTTP method of the REST API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// `GET`
    Get,
    /// `POST`
    Post,
    /// `PUT`
    Put,
    /// `DELETE`
    Delete,
}

impl Method {
    /// The method's name, e.g. `"GET"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
        }
    }
}

/// A REST API answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestResponse {
    /// The HTTP status.
    pub status: u16,
    /// The body: JSON, or PNG bytes for a rendered receipt.
    pub body: Vec<u8>,
}

impl RestResponse {
    /// The body as JSON (`null` if it is not JSON).
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }

    /// Whether the status is 2xx.
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The `{ error: { code, message } }` of an error answer.
    pub fn error(&self) -> (String, String) {
        let json = self.json();
        let error = &json["error"];
        let code = error["code"].as_str().unwrap_or("http_error").to_owned();
        let message = error["message"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("the server answered {}", self.status));
        (code, message)
    }
}

/// Sends requests to the REST API. `path` is relative to `/api/v1`, e.g. `/devices`.
///
/// ```
/// use hardless_mcp::{BoxFuture, Method, Rest, RestResponse};
/// struct Fixed;
/// impl Rest for Fixed {
///     fn send<'a>(&'a self, _: Method, _: &'a str, _: Option<serde_json::Value>)
///         -> BoxFuture<'a, Result<RestResponse, String>> {
///         Box::pin(async { Ok(RestResponse { status: 200, body: b"[]".to_vec() }) })
///     }
/// }
/// ```
pub trait Rest: Send + Sync + 'static {
    /// Sends one request. `Err` means the server could not be reached.
    fn send<'a>(
        &'a self,
        method: Method,
        path: &'a str,
        body: Option<Value>,
    ) -> BoxFuture<'a, Result<RestResponse, String>>;
}
