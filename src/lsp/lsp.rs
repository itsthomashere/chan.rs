pub mod handlers;
pub mod ioloop;
pub mod listener;
pub mod types;

use std::ops::DerefMut;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ioloop::io_loop::IoLoop;
use listener::listener::Listener;
use lsp_types::request::Initialize;
use lsp_types::{
    notification, request, CodeActionKind, InitializeParams, InitializeResult, ServerCapabilities,
};
use parking_lot::RwLock;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use types::types::{AdapterServerCapabilities, LanguageServerBinary, ProccessId};

pub struct LanguageSeverProcess {
    capabilities: RwLock<ServerCapabilities>,
    code_action_kinds: Option<Vec<CodeActionKind>>,
    io_loop: Arc<IoLoop>,
    listener: Arc<Listener>,
    output_rx: UnboundedReceiver<String>,
}
impl LanguageSeverProcess {
    pub fn new(binary: LanguageServerBinary, root_path: &Path, server_id: ProccessId) -> Self {
        let (request_tx, request_rx) = unbounded_channel::<String>();
        let (response_tx, response_rx) = unbounded_channel::<String>();
        let (output_tx, output_rx) = unbounded_channel::<String>();
        let listener = Listener::new(response_rx, request_tx, output_tx).unwrap();
        let io_loop = IoLoop::new(binary, server_id, root_path, response_tx, request_rx).unwrap();

        Self {
            capabilities: Default::default(),
            code_action_kinds: Default::default(),
            io_loop: Arc::new(io_loop),
            listener: Arc::new(listener),
            output_rx,
        }
    }

    pub async fn initialize(&self, params: InitializeParams) -> InitializeResult {
        Self::request::<Initialize>(self, params).await
    }

    pub async fn request<T: request::Request>(&self, params: T::Params) -> T::Result {
        self.listener.send_request::<T>(params).await.unwrap()
    }

    pub async fn notify<T: notification::Notification>(&self, params: T::Params) {
        self.listener.send_notification::<T>(params).await.unwrap()
    }

    pub fn name(&self) -> &str {
        self.io_loop.name()
    }

    pub fn server_id(&self) -> ProccessId {
        self.io_loop.server_id()
    }

    pub fn root_path(&self) -> &PathBuf {
        self.io_loop.root_path()
    }

    pub fn working_dir(&self) -> &PathBuf {
        self.io_loop.working_dir()
    }

    pub fn capabilities(&self) -> ServerCapabilities {
        self.capabilities.read().clone()
    }

    pub fn code_action_kinds(&self) -> Option<Vec<CodeActionKind>> {
        self.code_action_kinds.clone()
    }

    pub fn adapter_server_capabilities(&self) -> AdapterServerCapabilities {
        AdapterServerCapabilities {
            server_capabilities: self.capabilities(),
            code_action_kinds: self.code_action_kinds(),
        }
    }

    pub fn update_capabilities(&self, update: impl FnOnce(&mut ServerCapabilities)) {
        update(self.capabilities.write().deref_mut())
    }
}
