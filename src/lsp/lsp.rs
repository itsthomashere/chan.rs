use std::{
    collections::HashMap,
    ffi::OsString,
    path::PathBuf,
    sync::{atomic::AtomicI32, Arc},
};

use anyhow::Result;
use lsp_types::{CodeActionKind, ServerCapabilities};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};
use tokio::process::Child;

const JSON_RPC_VER: &str = "2.0";
const content_length_headers: &str = "Content-Length: ";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(untagged)]
enum RequestIdType {
    Int(i32),
    Str(String),
}

#[derive(Serialize, Deserialize)]
struct Request<'a, T> {
    jsonrpc: &'static str,
    id: RequestIdType,
    method: &'a str,
    params: T,
}

#[derive(Debug, Deserialize, Serialize)]
struct Error {
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum LspResult<T> {
    #[serde(rename = "result")]
    Ok(Option<T>),
    Error(Option<Error>),
}

#[derive(Deserialize, Serialize)]
struct AnyResponse<'a> {
    jsonrpc: &'a str,
    id: RequestIdType,
    #[serde(default)]
    error: Option<Error>,
    #[serde(borrow)]
    result: Option<&'a RawValue>,
}

#[derive(Serialize)]
struct Response<T> {
    jsonrpc: &'static str,
    id: RequestIdType,
    #[serde(flatten)]
    value: LspResult<T>,
}

#[derive(Deserialize, Debug, Clone)]
struct AnyNotification {
    #[serde(default)]
    id: Option<RequestIdType>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Serialize, Deserialize)]
struct Notification<'a, T> {
    jsonrpc: &'static str,
    #[serde(borrow)]
    method: &'a str,
    params: T,
}

// Defining basic language server
//
pub enum IoKind {
    StdIn,
    StdOut,
    StdErr,
}

pub struct LanguageServerBin {
    path: PathBuf,
    env: Vec<OsString>,
    args: Option<Vec<HashMap<String, String>>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct LspId(pub usize);

pub struct LanguageServer {
    server_id: LspId,
    next_id: AtomicI32,
    name: Arc<str>,
    capabilities: RwLock<ServerCapabilities>,
    code_action_kinds: Option<Vec<CodeActionKind>>,
    root_path: PathBuf,
    working_dir: PathBuf,
    // TODO: io_handlers
    // TODO: response_handlers
    // TODO: notification_handlers
    // TODO: using channel to handle tasks
    server: Arc<Mutex<Child>>,
}

pub struct AdapterServerCapabilities {
    pub server_capabilities: ServerCapabilities,
    pub code_action_kinds: Option<Vec<CodeActionKind>>,
}

impl LanguageServer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            server_id: todo!(),
            next_id: todo!(),
            name: todo!(),
            capabilities: todo!(),
            code_action_kinds: todo!(),
            root_path: todo!(),
            working_dir: todo!(),
            server: todo!(),
        })
    }
    pub fn initialize() -> {}
}
