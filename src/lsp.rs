use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

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

/// Implemetation of LSP Request Id
/// [See](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#requestMessage)
#[derive(Debug, Clone, Deserialize, Serialize)]
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
    pub jsonrpc: &'static str,
    pub id: RequestId,
    #[serde(flatten)]
    pub value: LSPResult<'a, T>,
}

// Result of response message
/// [Response Message](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#responseMessage)
#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum LSPResult<'a, T> {
    #[serde(rename = "result")]
    Ok(Option<T>),
    #[serde(borrow, rename = "error")]
    Err(Option<LSPError<'a>>),
}

/// Implementation of Response Error
/// [Response Error](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#responseError)
///
/// * `message`: Error message
/// * `code`: Error code
/// * `data`: LSPAny
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LSPError<'a> {
    pub message: String,
    pub code: i32,
    #[serde(borrow)]
    pub data: Option<&'a RawValue>,
}

/// Serialize any response we got back from the server
///
/// * `jsonrpc`: jsonrpc version, see [JSON_RPC_VERSION]
/// * `id`: Request Id, see [RequestId]
/// * `result`: response result field
/// * `error`: response error field
#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct AnyResponse<'a> {
    jsonrpc: &'a str,
    id: RequestId,
    #[serde(borrow)]
    result: Option<&'a RawValue>,
    #[serde(default)]
    error: Option<LSPError<'a>>,
}
