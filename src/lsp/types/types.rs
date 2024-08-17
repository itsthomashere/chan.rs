use std::{collections::HashMap, ffi::OsString, path::PathBuf, time::Duration};

use lsp_types::{CodeActionKind, ServerCapabilities};
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};

pub const CONTENT_LEN_HEADER: &str = "Content-Length: ";
pub const LSP_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
pub const JSONPRC_VER: &str = "2.0";
pub const HEADER_DELIMITER: &[u8; 4] = b"\r\n\r\n";
pub type NotificationHandler = Box<dyn Send + FnMut(Option<LspRequestId>, Value)>;
pub type ResponseHandler = Box<dyn Send + FnOnce(Result<String, Error>)>;
pub type IoHandler = Box<dyn Send + FnMut(IoKind, &str)>;

#[derive(Debug, Clone, Copy)]
pub enum IoKind {
    StdOut,
    StdIn,
    StdErr,
}
pub struct LanguageServerBinary {
    pub path: PathBuf,
    pub envs: Option<HashMap<String, String>>,
    pub args: Vec<OsString>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ProccessId(pub usize);

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LspRequestId {
    Str(String),
    Int(i32),
}

#[derive(Deserialize, Serialize)]
pub struct InternalLspRequest<'a, T> {
    pub jsonrpc: &'a str,
    pub id: LspRequestId,
    pub method: &'a str,
    pub params: T,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnyResponse<'a> {
    pub jsonrpc: &'a str,
    pub id: LspRequestId,
    #[serde(default)]
    pub error: Option<Error>,
    #[serde(borrow)]
    pub result: Option<&'a RawValue>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct LspResponse<'a, T> {
    jsonrpc: &'a str,
    id: LspRequestId,
    #[serde(flatten)]
    value: LspResult<T>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LspResult<T> {
    #[serde(rename = "result")]
    Ok(Option<T>),
    Error(Option<Error>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Error {
    pub message: String,
}
#[derive(Deserialize, Serialize)]
pub struct LspNotification<'a, T> {
    pub jsonrpc: &'a str,
    #[serde(borrow)]
    pub method: &'a str,
    pub params: T,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct AnyNotification {
    #[serde(default)]
    pub id: Option<LspRequestId>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

pub struct AdapterServerCapabilities {
    pub server_capabilities: ServerCapabilities,
    pub code_action_kinds: Option<Vec<CodeActionKind>>,
}
