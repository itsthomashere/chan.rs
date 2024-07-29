use core::fmt;
use std::{
    collections::HashMap,
    ffi::OsString,
    hash::Hash,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{atomic::AtomicI32, Arc},
};

use anyhow::Context;
use lsp_types::{CodeActionKind, ServerCapabilities};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    process::{self, Child},
    sync::{mpsc, oneshot},
};

const CONTENT_LEN_HEADERS: &str = "Content-Length: ";
const JSON_RPC_VERSION: &str = "2.0";
type NotificationHandler = Box<dyn Send + FnMut(Option<LspMessageId>, Value)>;
type ResponseHandler = Box<dyn Send + FnOnce(Result<String, Error>)>;
type IoHandler = Box<dyn Send + FnMut(IoKind, &str)>;

#[derive(Debug, Clone, Copy)]
pub enum IoKind {
    StdOut,
    StdIn,
    StdErr,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub enum LspMessageId {
    Int(i32),
    Str(String),
}

#[derive(Serialize, Deserialize)]
struct LspRequestMessage<'a, T> {
    jsonrpc: &'a str,
    id: LspMessageId,
    method: &'a str,
    params: T,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum LspResponseResult<T> {
    #[serde(rename = "result")]
    Ok(Option<T>),
    Error(Option<Error>),
}

#[derive(Debug, Serialize, Deserialize)]
struct Error {
    message: String,
}
#[derive(Serialize, Deserialize)]
struct AbstractResponse<'a> {
    jsonrpc: &'a str,
    #[serde(default)]
    id: Option<LspMessageId>,
    error: Option<Error>,
    #[serde(borrow)]
    result: Option<&'a RawValue>,
}

#[derive(Serialize)]
struct LspResponseMessage<T> {
    jsonrpc: &'static str,
    id: LspMessageId,
    #[serde(flatten)]
    value: LspResponseResult<T>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct AbstractNotification {
    #[serde(default)]
    id: Option<LspMessageId>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Serialize, Deserialize)]
pub struct LspNotificationMessage<'a, T> {
    jsonrpc: &'a str,
    #[serde(borrow)]
    method: &'a str,
    params: T,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LanguageServerBinary {
    pub path: PathBuf,
    pub envs: Option<HashMap<String, String>>,
    pub args: Vec<OsString>,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct LspId(pub usize);

pub struct LanguageServer {
    server_id: LspId,
    next_id: AtomicI32,
    name: Arc<str>,
    capabilities: RwLock<ServerCapabilities>,
    code_action_kinds: Option<Vec<CodeActionKind>>,
    notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
    response_handlers: Arc<Mutex<Option<HashMap<LspMessageId, ResponseHandler>>>>,
    io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
    root_path: PathBuf,
    working_dir: PathBuf,
    out_bound_tx: tokio::sync::mpsc::UnboundedSender<String>,
    output_done_rx: Mutex<Option<oneshot::Receiver<String>>>,
    server: Arc<Mutex<Option<Child>>>,
}

impl LanguageServer {
    pub fn new(
        binary: LanguageServerBinary,
        server_id: LspId,
        root_path: &Path,
        code_action_kinds: Option<Vec<CodeActionKind>>,
    ) -> anyhow::Result<Self> {
        let working_dir = if root_path.is_dir() {
            root_path
        } else {
            root_path.parent().unwrap_or_else(|| Path::new("/"))
        };

        log::info!(
            "Lsp starting. Path: {:?}, working directory: {:?}, args: {:?}",
            &binary.path,
            working_dir,
            &binary.args
        );

        let mut command = process::Command::new(&binary.path);
        command
            .current_dir(working_dir)
            .args(&binary.args)
            .envs(binary.envs.unwrap_or_default())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut server = command.spawn().with_context(|| {
            format!(
                "Failed to spawn command. Path: {:?}, working directory: {:?}, args: {:?}",
                &binary.path, working_dir, &binary.args
            )
        })?;

        let stdin = server.stdin.take().unwrap();
        let stdout = server.stdout.take().unwrap();
        let stderr = server.stderr.take().unwrap();

        let mut server = Self::initilize_internal(
            server_id,
            stdin,
            stdout,
            Some(stderr),
            Some(server),
            root_path,
            working_dir,
            code_action_kinds,
            move |notification| {
                log::info!(
                    "Lsp with id {} send unhandled notification{}:\n {}",
                    server_id,
                    notification.method,
                    serde_json::to_string_pretty(&notification.params).unwrap()
                )
            },
        );
        if let Some(name) = binary.path.file_name() {
            server.name = name.to_string_lossy().into()
        }

        Ok(server)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn initilize_internal<Stdin, Stdout, Stderr, F>(
        server_id: LspId,
        stdin: Stdin,
        stdout: Stdout,
        stderr: Option<Stderr>,
        server: Option<Child>,
        root_path: &Path,
        working_dir: &Path,
        code_action_kinds: Option<Vec<CodeActionKind>>,
        on_unhandled_notification: F,
    ) -> Self
    where
        Stdin: AsyncWrite + Unpin + Send + 'static,
        Stdout: AsyncRead + Unpin + Send + 'static,
        Stderr: AsyncRead + Unpin + Send + 'static,
        F: FnMut(AbstractNotification) + 'static + Send + Sync + Clone,
    {
        let (out_bound_tx, out_bound_rx) = mpsc::unbounded_channel();
        let (output_done_tx, output_done_rx) = oneshot::channel();
        let notification_handlers =
            Arc::new(Mutex::new(HashMap::<_, NotificationHandler>::default()));

        let response_handlers =
            Arc::new(Mutex::new(Some(HashMap::<_, ResponseHandler>::default())));

        let io_handlers = Arc::new(Mutex::new(HashMap::default()));

        Self {
            server_id,
            next_id: Default::default(),
            name: Arc::default(),
            capabilities: Default::default(),
            code_action_kinds,
            notification_handlers,
            response_handlers,
            io_handlers,
            root_path: root_path.to_path_buf(),
            working_dir: working_dir.to_path_buf(),
            out_bound_tx,
            output_done_rx: Mutex::new(Some(output_done_rx)),
            server: Arc::new(Mutex::new(server)),
        }
    }
    pub fn initilize_lsp() {}
    pub fn code_action_kinds() {}
    pub fn handle_input() {}
    pub fn handle_stderr() {}
    pub fn handle_output() {}
    pub fn shutdown() {}
    pub fn on_request() {}
    pub fn name() {}
    pub fn capabilities() {}
    pub fn update_capabilities() {}
    pub fn send_request() {}
}

impl fmt::Display for LspId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Debug for LanguageServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LanguageServer")
            .field("id", &self.server_id.0)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
