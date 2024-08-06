mod handlers;
pub mod types;

use crate::types::types::LanguageServerBinary;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::AtomicI32;
use std::{path::Path, sync::Arc};

use anyhow::{anyhow, Context, Ok};
use lsp_types::request::Initialize;
use lsp_types::{
    request::{self, Request},
    InitializeParams,
};
use lsp_types::{CodeActionKind, ServerCapabilities};
use parking_lot::{Mutex, RwLock};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::{io::BufWriter, process::ChildStdout};
use types::types::{
    LspRequest, LspRequestId, NotificationHandler, ResponseHandler, CONTENT_LEN_HEADER,
    HEADER_DELIMITER, JSONPRC_VER,
};

pub struct LanguageSeverProcess {
    name: Arc<str>,
    pub process: Arc<Mutex<Child>>,
    next_id: AtomicI32,
    capabilities: RwLock<ServerCapabilities>,
    response_handlers: Arc<Mutex<Option<HashMap<LspRequestId, ResponseHandler>>>>,
    notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
    code_action_kinds: Option<Vec<CodeActionKind>>,
    outbound_sender: UnboundedSender<String>,
    outbound_receiver: UnboundedReceiver<String>,
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
        let notification_handlers =
            Arc::new(Mutex::new(HashMap::<_, NotificationHandler>::default()));
        let response_handlers =
            Arc::new(Mutex::new(Some(HashMap::<_, ResponseHandler>::default())));

        let (outbound_sender, outbound_receiver) = mpsc::unbounded_channel::<String>();

        Self {
            name: Arc::default(),
            next_id: Default::default(),
            process: Arc::new(Mutex::new(process)),
            capabilities: Default::default(),
            response_handlers,
            notification_handlers,
            code_action_kinds,
            outbound_sender,
            outbound_receiver,
            root_path: root_path.to_path_buf(),
            working_dir: working_dir.to_path_buf(),
        }
    }

    pub async fn handle_incoming_msg() -> anyhow::Result<()> {
        Ok(())
    }
    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        let params = InitializeParams::default();
        self.request::<Initialize>(params).await?;
        Ok(())
    }

    pub async fn request<T: request::Request>(&mut self, params: T::Params) -> anyhow::Result<()> {
        let mut proc = self.process.lock();
        let stdin = proc.stdin.take().unwrap();

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

        let mut content_len_buffer = Vec::new();
        if let Err(msg) = write!(content_len_buffer, "{}", message.as_bytes().len()) {
            return Err(anyhow!("Failed to write content len into buffer: {}", msg));
        }

        let mut writer = BufWriter::new(stdin);

        writer.write_all(CONTENT_LEN_HEADER.as_bytes()).await?;
        writer.write_all(&content_len_buffer).await?;
        writer.write_all("\r\n\r\n".as_bytes()).await?;
        writer.write_all(message.as_bytes()).await?;
        writer.flush().await?;
        Ok(())
    }
}
pub async fn read_headers(
    reader: &mut BufReader<ChildStdout>,
    buffer: &mut Vec<u8>,
) -> anyhow::Result<()> {
    loop {
        if buffer.len() >= HEADER_DELIMITER.len()
            && buffer[(buffer.len() - HEADER_DELIMITER.len())..] == HEADER_DELIMITER[..]
        {
            return Ok(());
        }
        if reader.read_until(b'\n', buffer).await? == 0 {
            return Err(anyhow!("cannot read headers from stdout"));
        }
    }
}
