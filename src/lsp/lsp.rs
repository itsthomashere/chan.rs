use std::{
    collections::HashMap,
    ffi::OsString,
    path::PathBuf,
    sync::{Arc, Weak},
};

use anyhow::Result;
use lsp_types::{CodeActionKind, ServerCapabilities};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};
use tokio::process::Child;

const JSON_RPC_VER: &str = "2.0";
const CONTENT_LENGTH_HEADERS: &str = "Content-Length: ";

type NotificaionHandler = Box<dyn Send + FnMut(Option<RequestId>, Value)>;
type ResponseHandler = Box<dyn Send + FnOnce(Result<String, Error>)>;
type IoHandler = Box<dyn Send + FnMut(IoKind, &str)>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RequestId {
    Int(i32),
    Str(String),
}

pub enum Subscription {
    Notification {
        method: &'static str,
        notification_handlers: Option<Arc<Mutex<HashMap<&'static str, NotificaionHandler>>>>,
    },

    Io {
        id: i32,
        io_handers: Option<Weak<Mutex<HashMap<i32, IoHandler>>>>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct Error {
    message: String,
}

#[derive(Debug, Clone, Copy)]
pub enum IoKind {
    StdIn,
    StdOut,
    StdErr,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LspRequest<'a, T> {
    jsonrpc: &'static str,
    id: RequestId,
    method: &'a str,
    params: T,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum LspResult<T> {
    #[serde(rename = "result")]
    Ok(Option<T>),
    Error(Option<Error>),
}

#[derive(Serialize)]
struct LspResponse<T> {
    jsonrpc: &'static str,
    id: RequestId,
    #[serde(flatten)]
    value: LspResult<T>,
}

#[derive(Serialize, Deserialize)]
struct LspNotification<'a, T> {
    jsonrpc: &'static str,
    #[serde(borrow)]
    method: &'a str,
    params: T,
}

#[derive(Deserialize, Serialize)]
struct AnyResponse<'a> {
    jsonrpc: &'a str,
    id: RequestId,
    #[serde(default)]
    error: Option<Error>,
    #[serde(borrow)]
    result: Option<&'a RawValue>,
}

#[derive(Debug, Clone, Serialize)]
struct AnyNotification {
    #[serde(default)]
    id: RequestId,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LanguageServerBinary {
    path: PathBuf,
    envs: Option<HashMap<String, String>>,
    args: Vec<OsString>,
}
pub struct LanguageServer {
    name: Arc<str>,
    root_path: PathBuf,
    working_dir: PathBuf,
    capabilities: RwLock<ServerCapabilities>,
    notification_handlers: Arc<Mutex<HashMap<&'static str, NotificaionHandler>>>,
    response_handlers: Arc<Mutex<Option<HashMap<&'static str, ResponseHandler>>>>,
    io_handers: Arc<Mutex<HashMap<i32, IoHandler>>>,
    code_action_kinds: Option<Vec<CodeActionKind>>,
    server: Arc<Mutex<Option<Child>>>,
}

impl LanguageServer {
    fn initialize() {
        print!("initialize the LanguageServer");
    }

    fn code_action_kinds() {
        println!("Get code actions from the server")
    }

    fn capabilities() {
        println!("Get all get_capabilities of the LanguageServer");
    }

    fn update_capabilities() {
        println!("update capabilities");
    }

    fn name() {
        println!("LSP name");
    }

    fn server_id() {
        println!("LSP id")
    }

    fn root_path() {
        println!("get the root path")
    }

    fn adapter_capabilities() {
        println!("Return the shared client/adapter")
    }

    fn handle_stdin() {}

    fn handle_stderr() {}

    fn handle_stdout() {}

    fn handle_request() {
        println!("Register a handler for upcoming requests")
    }

    fn send_request() {
        print!("Send request is not implemented");
    }

    fn send_notification() {
        println!("this send the notification to the LanguageServer")
    }

    fn handle_notification() {
        println!("Register a handler for upcoming notification")
    }

    fn handle_io() {
        println!("Register a handler for io operation")
    }

    fn shutdown() {
        println!("Prepare to drop the server")
    }
}
