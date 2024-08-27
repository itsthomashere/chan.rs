use std::{collections::HashMap, ffi::OsString, path::PathBuf, sync::Arc, time::Duration};

use lsp_types::{CodeActionKind, ServerCapabilities};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};

pub const CONTENT_LEN_HEADER: &str = "Content-Length: ";
pub const LSP_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
pub const JSON_RPC_VER: &str = "2.0";
pub const HEADER_DELIMITER: &[u8; 4] = b"\r\n\r\n";

pub type NotificationHandler = Box<dyn Send + FnMut(Option<RequestId>, Value)>;
pub type ResponseHandler = Box<dyn Send + FnOnce(Result<String, LspError>)>;
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
pub enum RequestId {
    Str(String),
    Int(i32),
}

#[derive(Deserialize, Serialize)]
pub struct Request<'a, T> {
    pub jsonrpc: &'a str,
    pub id: RequestId,
    pub method: &'a str,
    pub params: T,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnyResponse<'a> {
    pub jsonrpc: &'a str,
    pub id: RequestId,
    #[serde(default)]
    pub error: Option<LspError>,
    #[serde(borrow)]
    pub result: Option<&'a RawValue>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Response<'a, T> {
    pub(crate) jsonrpc: &'a str,
    pub(crate) id: RequestId,
    #[serde(flatten)]
    pub(crate) value: LspResult<T>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LspResult<T> {
    #[serde(rename = "result")]
    Ok(Option<T>),
    Error(Option<LspError>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LspError {
    pub message: String,
}

#[derive(Deserialize, Serialize)]
pub struct Notification<'a, T> {
    pub jsonrpc: &'a str,
    #[serde(borrow)]
    pub method: &'a str,
    pub params: T,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct AnyNotification {
    #[serde(default)]
    pub id: Option<RequestId>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

pub struct AdapterServerCapabilities {
    pub server_capabilities: ServerCapabilities,
    pub code_action_kinds: Option<Vec<CodeActionKind>>,
}

pub enum Subscription {
    Notification {
        method: &'static str,
        notification_handlers: Option<Arc<Mutex<HashMap<&'static str, NotificationHandler>>>>,
    },
}
