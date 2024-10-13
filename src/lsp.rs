pub(crate) mod io;
pub(crate) mod listener;
pub mod process;
use std::time::Duration;

pub use lsp_types;
pub(crate) mod utils;
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};

/// The base Language Server Protocol consists of content part and header part
/// Header have 2 fields: Content-Length and Content-Type( Optional)
/// Header part is ascii encoded
/// Header and content part is seperated by \r\n\r\n

// The version used by most Language server is 2.0
pub const JSON_RPC_VERSION: &str = "2.0";

// Content length header
pub(crate) const CONTENT_LEN_HEADER: &str = "Content-Length: ";

// Header and content seperator
pub(crate) const HEADER_DELIMITER: &[u8; 4] = b"\r\n\r\n";

pub(crate) const LSP_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Implemetation of LSP Request Id
/// [See](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#requestMessage)
#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq, Hash)]
#[serde(untagged)]
pub enum RequestId {
    Int(i32),
    Str(String),
}

/// Implemetation of LSP Request Message
/// [Request Message](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#requestMessage)
///
/// * `jsonrpc`: jsonrpc version, see [JSON_RPC_VERSION]
/// * `id`: Request id, either integer or string
/// * `method`: Request method
/// * `params`: Request params
#[derive(Debug, Serialize, Clone)]
pub struct LSPRequest<'a, T> {
    pub jsonrpc: &'static str,
    pub id: RequestId,
    pub method: &'a str,
    pub params: T,
}

/// Implementation of LSP Notification Message
/// [Notification Message](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#notificationMessage)
///
/// * `jsonrpc`: jsonrpc version, see [JSON_RPC_VERSION]
/// * `method`: Notification Method
/// * `params`: Notification Parameters
#[derive(Debug, Serialize, Clone)]
pub struct LSPNotification<'a, T> {
    pub jsonrpc: &'static str,
    pub method: &'a str,
    pub params: T,
}

/// Implementation of LSPResponse message
/// [Response Message](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#responseMessage)
/// The response message required that result and error field can't be present on one message
///
/// * `jsonrpc`: jsonrpc version, see [JSON_RPC_VERSION]
/// * `id`: Request Id, see [RequestId]
/// * `value`: See [LSPResult]
#[derive(Debug, Deserialize, Clone)]
pub struct LSPResponse<'a, T> {
    pub jsonrpc: &'a str,
    pub id: RequestId,
    #[serde(flatten)]
    pub value: LSPResult<T>,
}

// Result of response message
/// [Response Message](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#responseMessage)
#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum LSPResult<T> {
    #[serde(rename = "result")]
    Ok(Option<T>),
    #[serde(rename = "error")]
    Err(Option<LSPError>),
}

/// Implementation of Response Error
/// [Response Error](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#responseError)
///
/// * `message`: Error message
/// * `code`: Error code
/// * `data`: LSPAny
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LSPError {
    pub message: String,
    pub code: i32,
    pub data: Option<Value>,
}

/// Serialize any response we got back from the server
///
/// * `jsonrpc`: jsonrpc version, see [JSON_RPC_VERSION]
/// * `id`: Request Id, see [RequestId]
/// * `result`: response result field
/// * `error`: response error field
#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct AnyResponse<'a> {
    pub(crate) jsonrpc: &'a str,
    pub(crate) id: RequestId,
    #[serde(borrow)]
    pub(crate) result: Option<&'a RawValue>,
    #[serde(default)]
    pub(crate) error: Option<LSPError>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct AnyNotification {
    pub(crate) method: String,
    #[serde(default)]
    pub(crate) id: Option<RequestId>,
    #[serde(default)]
    pub(crate) params: Option<Value>,
}
