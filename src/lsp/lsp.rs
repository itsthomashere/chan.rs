use std::{
    collections::HashMap,
    ops::DerefMut,
    path::{Path, PathBuf},
    sync::Arc,
};

use ioloop::io_loop::IoLoop;
use listener::listener::Listener;
use lsp_types::{notification, request, CodeActionKind, ServerCapabilities};
use parking_lot::{Mutex, RwLock};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use types::types::{
    AdapterServerCapabilities, AnyNotification, IoHandler, LanguageServerBinary,
    NotificationHandler, ProccessId, ResponseHandler,
};

pub mod handlers;
pub mod ioloop;
pub mod listener;
pub mod types;
pub mod util;

pub struct LanguageServerProcess {
    io_loop: Arc<IoLoop>,
    listener: Arc<Listener>,
    pub output_done_rx: UnboundedReceiver<String>,
    code_action_kind: Option<Vec<CodeActionKind>>,
    capabilities: RwLock<ServerCapabilities>,
}

impl LanguageServerProcess {
    pub fn new(
        binary: LanguageServerBinary,
        server_id: ProccessId,
        root_path: &Path,
        stderr_capture: Arc<Mutex<Option<String>>>,
        code_action_kind: Option<Vec<CodeActionKind>>,
    ) -> anyhow::Result<Self> {
        let (request_out_tx, request_in_rx) = unbounded_channel::<String>();
        let (notification_channel_tx, notification_channel_rx) =
            unbounded_channel::<AnyNotification>();
        let (output_done_tx, output_done_rx) = unbounded_channel();

        let notification_handlers =
            Arc::new(Mutex::new(HashMap::<_, NotificationHandler>::default()));
        let response_handlers =
            Arc::new(Mutex::new(Some(HashMap::<_, ResponseHandler>::default())));
        let io_handlers = Arc::new(Mutex::new(HashMap::<_, IoHandler>::default()));

        let io_loop = IoLoop::new(
            server_id,
            binary,
            root_path,
            io_handlers.clone(),
            response_handlers.clone(),
            notification_channel_tx,
            request_in_rx,
            output_done_tx,
            stderr_capture,
        )?;

        let listener = Listener::new(
            io_handlers,
            response_handlers,
            notification_handlers,
            notification_channel_rx,
            request_out_tx,
        )?;

        Ok(Self {
            io_loop: Arc::new(io_loop),
            listener: Arc::new(listener),
            output_done_rx,
            code_action_kind,
            capabilities: Default::default(),
        })
    }

    pub async fn request<T: request::Request>(
        &self,
        params: T::Params,
    ) -> anyhow::Result<T::Result> {
        self.listener.request::<T>(params).await
    }

    pub async fn notify<T: notification::Notification>(
        &self,
        params: T::Params,
    ) -> anyhow::Result<()> {
        self.listener.send_notification::<T>(params).await
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

    pub fn name(&self) -> &str {
        self.io_loop.name()
    }

    pub fn code_action_kind(&self) -> Option<Vec<CodeActionKind>> {
        self.code_action_kind.clone()
    }

    pub fn update_capabilities(&self, update: impl FnOnce(&mut ServerCapabilities)) {
        update(self.capabilities.write().deref_mut())
    }

    pub fn capabilities(&self) -> ServerCapabilities {
        self.capabilities.read().clone()
    }

    pub fn adapter_capabilities(&self) -> AdapterServerCapabilities {
        AdapterServerCapabilities {
            server_capabilities: self.capabilities(),
            code_action_kinds: self.code_action_kind(),
        }
    }

    pub fn force_kill(&mut self) -> anyhow::Result<()> {
        self.io_loop.kill()?;
        self.listener.kill()?;
        self.output_done_rx.close();
        Ok(())
    }
}
