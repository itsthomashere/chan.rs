mod input_handlers;
mod io_loop;
mod listener;
pub mod types;
pub mod util;
pub use lsp_types;

use std::{
    collections::HashMap,
    future::Future,
    ops::DerefMut,
    path::{Path, PathBuf},
    sync::Arc,
};

use io_loop::IoLoop;
use listener::Listener;
use lsp_types::{notification, request, CodeActionKind, ServerCapabilities};
use parking_lot::{Mutex, RwLock};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use types::{
    AdapterServerCapabilities, AnyNotification, IoHandler, IoKind, LanguageServerBinary,
    NotificationHandler, ProccessId, ResponseHandler, Subscription,
};

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

    pub fn on_notification<T: notification::Notification, F>(&self, f: F) -> Subscription
    where
        F: 'static + Send + FnMut(T::Params),
    {
        self.listener.on_notification::<T, F>(f)
    }

    pub fn on_request<T: request::Request, Fut, F>(&self, f: F) -> Subscription
    where
        Fut: 'static + Future<Output = anyhow::Result<T::Result>> + Send,
        F: 'static + Send + FnMut(T::Params) -> Fut + Send,
    {
        self.listener.on_request::<T, Fut, F>(f)
    }

    pub fn on_io<F>(&self, f: F) -> Subscription
    where
        F: 'static + Send + FnMut(IoKind, &str),
    {
        self.listener.on_io(f)
    }

    pub fn remove_notification_handler<T: notification::Notification>(&self) {
        self.listener.remove_notification_handler::<T>();
    }

    pub fn remove_request_handler<T: request::Request>(&self) {
        self.listener.remove_request_handler::<T>();
    }

    pub fn has_notification_handler<T: notification::Notification>(&self) -> bool {
        self.listener.has_notification_handler::<T>()
    }

    pub fn has_request_handler<T: request::Request>(&self) -> bool {
        self.listener.has_request_handler::<T>()
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
