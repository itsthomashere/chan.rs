pub mod handlers;
pub mod ioloop;
pub mod listener;
pub mod types;

use std::ops::DerefMut;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ioloop::io_loop::IoLoop;
use listener::listener::Listener;
use lsp_types::{CodeActionKind, ServerCapabilities};
use parking_lot::RwLock;
use tokio::sync::mpsc::unbounded_channel;
use types::types::{AdapterServerCapabilities, LanguageServerBinary, ProccessId};

pub struct LanguageSeverProcess {
    capabilities: RwLock<ServerCapabilities>,
    code_action_kinds: Option<Vec<CodeActionKind>>,
    io_loop: Arc<IoLoop>,
    listener: Arc<Listener>,
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
        }
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
