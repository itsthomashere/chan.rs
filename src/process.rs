use std::{
    borrow::BorrowMut,
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
};

use lsp_types::{notification, request, CodeActionKind, ServerCapabilities};
use parking_lot::{Mutex, RwLock};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::{
    io::{IoHandler, NotificationHandler, ResponseHandler, IO},
    listener::Listener,
    AnyNotification,
};

pub struct LanguageServerBinary {
    pub path: PathBuf,
    pub envs: Option<HashMap<String, String>>,
    pub args: Vec<OsString>,
}

pub struct LanguageServer {
    io: IO,
    listener: Listener,
    pub output_done_rx: UnboundedReceiver<String>,
    code_action_kind: Option<Vec<CodeActionKind>>,
    capabilities: RwLock<ServerCapabilities>,
}

impl LanguageServer {
    pub fn new(
        binary: LanguageServerBinary,
        id: i32,
        root_path: &Path,
        capture: Arc<Mutex<Option<String>>>,
        code_action_kind: Option<Vec<CodeActionKind>>,
    ) -> anyhow::Result<Self> {
        let (request_tx, request_rx) = unbounded_channel::<String>();
        let (notification_tx, notification_rx) = unbounded_channel::<AnyNotification>();
        let (output_done_tx, output_done_rx) = unbounded_channel();

        let notification_handlers =
            Arc::new(Mutex::new(HashMap::<_, NotificationHandler>::default()));
        let response_handlers =
            Arc::new(Mutex::new(Some(HashMap::<_, ResponseHandler>::default())));

        let io_handlers = Arc::new(Mutex::new(HashMap::<_, IoHandler>::default()));

        let io = IO::new(
            id,
            binary,
            response_handlers.clone(),
            io_handlers.clone(),
            request_rx,
            notification_tx,
            output_done_tx,
            root_path,
            capture,
        )?;
        let listener = Listener::new(
            notification_rx,
            notification_handlers,
            response_handlers,
            io_handlers,
            request_tx,
        )?;

        Ok(Self {
            io,
            listener,
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

    pub fn on_request(&self) {
        self.listener.on_request()
    }

    pub fn on_notification(&self) {
        self.listener.on_notification()
    }

    pub fn on_io(&self) {
        self.listener.on_io()
    }

    pub fn server_id(&self) -> i32 {
        self.io.id()
    }

    pub fn root_path(&self) -> &PathBuf {
        self.io.root_path()
    }

    pub fn working_dir(&self) -> &PathBuf {
        self.io.working_dir()
    }

    pub fn capabilities(&self) -> ServerCapabilities {
        self.capabilities.read().clone()
    }

    pub fn update_capabilities(&mut self) {}

    pub fn code_action_kinds(&self) -> Option<Vec<CodeActionKind>> {
        self.code_action_kind.clone()
    }

    pub fn name(&self) -> &str {
        self.io.name()
    }

    pub fn kill(&mut self) -> anyhow::Result<()> {
        self.io.borrow_mut().kill()?;
        self.listener.kill()?;
        Ok(())
    }
}
