//! A small client for a running server's REST API (`/api/v1`), for the CLI's commands.

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::header::{AUTHORIZATION, CONTENT_TYPE};
use hyper::{Method, Request, Uri};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client as HyperClient;
use hyper_util::rt::TokioExecutor;
use serde_json::Value;

/// Why a command failed.
#[derive(Debug)]
pub enum ClientError {
    /// The server could not be reached.
    Unreachable(String),
    /// The server answered with an error.
    Api {
        /// Its message.
        message: String,
    },
    /// Anything else: a bad URL, an unreadable answer.
    Other(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Unreachable(server) => write!(
                f,
                "cannot reach the Hardless server at {server}. Is `hardless start` running? (--server or HARDLESS_SERVER sets the address)"
            ),
            ClientError::Api { message, .. } => f.write_str(message),
            ClientError::Other(message) => f.write_str(message),
        }
    }
}

/// A connection to a server.
pub struct Client {
    base: String,
    token: Option<String>,
    http: HyperClient<HttpConnector, Full<Bytes>>,
}

impl Client {
    /// A client for the server at `server` (`http://host:port`).
    pub fn new(server: &str, token: Option<String>) -> Result<Self, ClientError> {
        let base = server.trim_end_matches('/').to_owned();
        let uri: Uri = base
            .parse()
            .map_err(|_| ClientError::Other(format!("not a URL: {server}")))?;
        if uri.scheme_str() != Some("http") {
            return Err(ClientError::Other(format!(
                "use the server's http:// address (port 8080 by default), not {server}"
            )));
        }
        Ok(Self {
            base,
            token: token.filter(|t| !t.is_empty()),
            http: HyperClient::builder(TokioExecutor::new()).build_http(),
        })
    }

    /// Sends a request to `/api/v1<path>` and returns the status and body.
    pub async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Bytes), ClientError> {
        let uri = format!("{}/api/v1{path}", self.base);
        let mut request = Request::builder().method(method).uri(&uri);
        if let Some(token) = &self.token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        let payload = match body {
            Some(value) => {
                request = request.header(CONTENT_TYPE, "application/json");
                Bytes::from(value.to_string())
            }
            None => Bytes::new(),
        };
        let request = request
            .body(Full::new(payload))
            .map_err(|e| ClientError::Other(e.to_string()))?;
        let response = self
            .http
            .request(request)
            .await
            .map_err(|_| ClientError::Unreachable(self.base.clone()))?;
        let status = response.status().as_u16();
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|e| ClientError::Other(format!("reading the answer failed: {e}")))?
            .to_bytes();
        Ok((status, bytes))
    }

    /// Like [`Client::send`], turning error answers into [`ClientError::Api`].
    pub async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Bytes, ClientError> {
        let (status, bytes) = self.send(method, path, body).await?;
        if (200..300).contains(&status) {
            return Ok(bytes);
        }
        let error: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        let message = error["error"]["message"]
            .as_str()
            .map_or_else(|| format!("the server answered {status}"), str::to_owned);
        Err(ClientError::Api { message })
    }

    /// [`Client::call`], parsing the answer as JSON.
    pub async fn json(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, ClientError> {
        let bytes = self.call(method, path, body).await?;
        serde_json::from_slice(&bytes)
            .map_err(|e| ClientError::Other(format!("the server's answer is not JSON: {e}")))
    }
}
