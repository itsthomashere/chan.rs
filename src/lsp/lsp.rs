pub mod handlers;
pub mod types;

use crate::types::types::LanguageServerBinary;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::AtomicI32;
use std::{path::Path, sync::Arc};

use anyhow::{anyhow, Context};
use handlers::input_handlers::read_headers;
use lsp_types::request::Initialize;
use lsp_types::{
    request::{self},
    InitializeParams,
};
use lsp_types::{CodeActionKind, ServerCapabilities};
use parking_lot::{Mutex, RwLock};
use tokio::io::AsyncWriteExt;
use tokio::io::{BufReader, BufWriter};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use types::types::{LspRequest, LspRequestId, CONTENT_LEN_HEADER, JSONPRC_VER};

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
        process: Child,
        root_path: &Path,
        working_dir: &Path,
        code_action_kinds: Option<Vec<CodeActionKind>>,
    ) -> Self {
        let (request_tx, response_rx) = mpsc::unbounded_channel::<String>();
        let (notification_tx, notification_rx) = mpsc::unbounded_channel::<String>();
        let (err_tx, err_rx) = mpsc::unbounded_channel::<String>();

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

        self.outbound_sender.send(message)?;
        self.handle_channel_in().await?;

        Ok(())
    }

    async fn register_output_chanel(
        &mut self,
        channel_out: UnboundedSender<String>,
    ) -> anyhow::Result<()> {
        let mut proc = self.process.clone();
        let mut stdout = proc.lock().stdout.take().unwrap();
        let mut reader = BufReader::new(stdout);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            read_headers(&mut reader, &mut buffer).await?;
        }
    }

    async fn handle_channel_in(&mut self) -> anyhow::Result<()> {
        let mut proc = self.process.clone();
        let stdin = proc.lock().stdin.take().unwrap();

        let mut writer = BufWriter::new(stdin);

        let mut content_len_buffer: Vec<u8> = Vec::new();

        if let Some(msg) = self.outbound_receiver.recv().await {
            content_len_buffer.clear();
            if let Err(msg) = write!(content_len_buffer, "{}", msg.as_bytes().len()) {
                return Err(anyhow!("Failed to write content len into buffer: {}", msg));
            }
            writer.write_all(CONTENT_LEN_HEADER.as_bytes()).await?;
            writer.write_all(&content_len_buffer).await?;
            writer.write_all("\r\n\r\n".as_bytes()).await?;
            writer.write_all(msg.as_bytes()).await?;
            writer.flush().await?;
        }
        tokio::task::yield_now().await;

        Ok(())
    }
}
