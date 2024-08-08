pub mod handlers;
pub mod types;

use crate::types::types::LanguageServerBinary;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::AtomicI32;
use std::{path::Path, sync::Arc};

use anyhow::{anyhow, Context};
use handlers::input_handlers::read_headers;
use log::warn;
use lsp_types::request::Initialize;
use lsp_types::{
    request::{self},
    InitializeParams,
};
use lsp_types::{CodeActionKind, ServerCapabilities};
use parking_lot::{Mutex, RwLock};
use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::io::BufReader;
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use types::types::{
    AnyNotification, AnyResponse, LspRequest, LspRequestId, LspResponse, NotificationHandler,
    ResponseHandler, CONTENT_LEN_HEADER, JSONPRC_VER,
};

pub struct LanguageSeverProcess {
    name: Arc<str>,
    pub process: Arc<Mutex<Child>>,
    next_id: AtomicI32,
    capabilities: RwLock<ServerCapabilities>,
    code_action_kinds: Option<Vec<CodeActionKind>>,
    request_tx: UnboundedSender<String>,
    response_rx: UnboundedReceiver<String>,
    err_rx: UnboundedReceiver<String>,
    notification_tx: UnboundedSender<String>,
    notification_rx: UnboundedReceiver<String>,
    root_path: PathBuf,
    working_dir: PathBuf,
    response_handlers: Arc<Mutex<Option<HashMap<LspRequestId, ResponseHandler>>>>,
    notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
}
impl LanguageSeverProcess {
    pub fn new(
        binary: LanguageServerBinary,
        root_path: &Path,
        code_action_kinds: Option<Vec<CodeActionKind>>,
    ) -> anyhow::Result<Self> {
        let working_dir = if root_path.is_dir() {
            root_path
        } else {
            root_path.parent().unwrap_or_else(|| Path::new("/"))
        };

        let mut command = Command::new(&binary.path);
        command
            .current_dir(working_dir)
            .envs(binary.envs.unwrap_or_default())
            .args(&binary.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let process = command.spawn().with_context(|| {
            format!(
                "failed to spawn command. path: {:?}, working directory: {:?}, args: {:?}",
                binary.path, working_dir, &binary.args
            )
        })?;

        let mut server = Self::binding_backend(process, root_path, working_dir, code_action_kinds);

        if let Some(name) = binary.path.file_name() {
            server.name = name.to_string_lossy().into()
        };

        Ok(server)
    }

    fn binding_backend(
        mut process: Child,
        root_path: &Path,
        working_dir: &Path,
        code_action_kinds: Option<Vec<CodeActionKind>>,
    ) -> Self {
        let (request_tx, response_rx) = mpsc::unbounded_channel::<String>();
        let (notification_tx, notification_rx) = mpsc::unbounded_channel::<String>();
        let (err_tx, err_rx) = mpsc::unbounded_channel::<String>();
        let response_handlers =
            Arc::new(Mutex::new(Some(HashMap::<_, ResponseHandler>::default())));

        let notification_handlers =
            Arc::new(Mutex::new(HashMap::<_, NotificationHandler>::default()));

        let stdout = process.stdout.take().unwrap();
        Self::register_processe_stdout_handlers(
            stdout,
            &response_rx,
            response_handlers.clone(),
            notification_handlers.clone(),
        );

        Self {
            name: Arc::default(),
            next_id: Default::default(),
            process: Arc::new(Mutex::new(process)),
            capabilities: Default::default(),
            code_action_kinds,
            root_path: root_path.to_path_buf(),
            working_dir: working_dir.to_path_buf(),
            request_tx,
            response_rx,
            err_rx,
            notification_tx,
            notification_rx,
            response_handlers,
            notification_handlers,
        }
    }

    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        let params = InitializeParams::default();
        self.send_request::<Initialize>(params).await?;
        Ok(())
    }

    pub async fn send_request<T: request::Request>(
        &mut self,
        params: T::Params,
    ) -> anyhow::Result<()> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let message = serde_json::to_string(&LspRequest {
            jsonrpc: JSONPRC_VER,
            id: LspRequestId::Int(id),
            method: T::METHOD,
            params,
        })
        .unwrap();
        println!("{}", message);

        self.request_tx.send(message)?;

        Ok(())
    }

    async fn register_processe_stdout_handlers(
        stdout: ChildStdout,
        response_rx: &UnboundedReceiver<String>,
        response_handlers: Arc<Mutex<Option<HashMap<LspRequestId, ResponseHandler>>>>,
        notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
    ) -> anyhow::Result<()> {
        let mut reader = BufReader::new(stdout);
        let mut buffer: Vec<u8> = Vec::new();

        loop {
            buffer.clear();
            read_headers(&mut reader, &mut buffer).await?;
            let header = std::str::from_utf8(&buffer)?;
            let message_len: usize = header
                .split('\n')
                .find(|line| line.starts_with(CONTENT_LEN_HEADER))
                .and_then(|line| line.strip_prefix(CONTENT_LEN_HEADER))
                .ok_or_else(|| anyhow!("Invalid headers"))?
                .trim_end()
                .parse::<usize>()?;

            buffer.resize(message_len, 0);

            reader.read_exact(&mut buffer).await?;

            if let Ok(msg) = std::str::from_utf8(&buffer) {
                log::trace!("Incoming lsp message {}", msg);
                continue;
            }

            if let Ok(notification) = serde_json::from_slice::<AnyNotification>(&buffer) {
                let mut notification_handlers = notification_handlers.lock();
                if let Some(handler) = notification_handlers.get_mut(notification.method.as_str()) {
                    handler(notification.id, notification.params.unwrap_or(Value::Null));
                } else {
                    drop(notification_handlers);
                    warn!("Unhandled notification");
                }
            } else if let Ok(AnyResponse {
                id, result, error, ..
            }) = serde_json::from_slice(&buffer)
            {
                let mut response_handlers = response_handlers.lock();

                if let Some(handler) = response_handlers
                    .as_mut()
                    .and_then(|handler| handler.remove(&id))
                {
                    drop(response_handlers);
                    if let Some(err) = error {
                        handler(Err(err));
                    } else if let Some(result) = result {
                        handler(Ok(result.get().into()));
                    } else {
                        handler(Ok("Null".into()));
                    }
                }
            } else {
                warn!(
                    "Failed to deserialize lsp message: \n{}",
                    std::str::from_utf8(&buffer)?
                );
            }
        }
    }
    async fn register_error_handlers(&mut self) {}
    async fn register_notification_handlers(&mut self) {}
    async fn get_notification() {}
}
