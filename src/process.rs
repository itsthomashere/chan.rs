use std::{
    collections::HashMap,
    ffi::OsString,
    future::Future,
    ops::DerefMut,
    path::{Path, PathBuf},
    sync::Arc,
};

use lsp_types::{notification, request, CodeActionKind, ServerCapabilities};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::IOKind;
use crate::{
    io::{IoHandler, NotificationHandler, ResponseHandler, IO},
    listener::Listener,
    utils::Subscription,
    AnyNotification,
};

/// Binary of the language server
///
/// * `path`: path to the executable
/// * `envs`: List of environment variables
/// * `args`: List of arguments for starting the process
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
    /// Start a new language server process
    /// A process is construct by one io_listener and one listener_loop
    /// When sending something to the process, the request will be handled by background task
    /// keeping the process lock-free
    ///
    /// # Usage
    /// ``` rust
    ///     let binary: LanguageServerBinary = LanguageServerBinary { ... };
    ///     let root_path = Path::new("your-root");
    ///     // Stderr capture will take every stderr response receivered
    ///     // usefull for logging
    ///     let stderr_capture = Arc::new(Mutex::new(...))
    ///     let server =
    ///         LanguageServerProcess::new(binary, 1, root_path, stderr_capture, None)?;
    /// ```
    /// * `binary`: See [LanguageServerBinary]
    /// * `id`: id for the server
    /// * `root_path`: Root path for the lsp, useful for discovering workspaces
    /// * `capture`: Stderr capturer
    /// * `code_action_kind`: List of code action kinds that will be registered during startup
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

    /// Send a request to the server and get the response back
    /// T must be type of [request::Request]. We had re-exported the module
    ///
    /// # Usage
    /// ```rust
    ///     use chan_rs::lsp_types::request::Initialize;
    ///
    ///     let init_params = IntializeParams::default();
    ///     let response = server.request::<Initialize>(init_params)?;
    /// ```
    /// * `params`: Parameters for the request
    pub async fn request<T: request::Request>(
        &self,
        params: T::Params,
    ) -> anyhow::Result<T::Result> {
        self.listener.request::<T>(params).await
    }

    /// Send a notify to the server, notify requests don't send response back
    /// T must be type of [notification::Notification]. We had re-exported the module
    ///
    /// # Usage
    /// ```rust
    ///     use chan_rs::lsp_types::notification::Initialized;
    ///
    ///     let initialized = IntializedParams::default();
    ///     server.notify::<Initialized>(initialized)?;
    /// ```
    ///
    /// * `params`: Parameters for the notification
    pub async fn notify<T: notification::Notification>(
        &self,
        params: T::Params,
    ) -> anyhow::Result<()> {
        self.listener.send_notification::<T>(params).await
    }

    ///
    /// Register a handler to handle incoming request
    /// You can only register on handler for one method
    pub fn on_request<T: request::Request, F, Fut, Res>(&self, f: F) -> Subscription
    where
        T::Params: 'static + Send,
        F: Send + 'static + FnMut(T::Params) -> Fut,
        Fut: Send + 'static + Future<Output = anyhow::Result<Res>>,
        Res: Serialize,
    {
        self.listener.on_request::<T, F, Fut, Res>(f)
    }

    ///
    /// Register a handler to handle incoming notification
    /// You can only register one handler for one method
    pub fn on_notification<T: notification::Notification, F>(&self, f: F) -> Subscription
    where
        T::Params: 'static + Send,
        F: Send + 'static + FnMut(T::Params),
    {
        self.listener.on_notification::<T, F>(f)
    }

    /// Register a handler to handle stdio
    /// You can re-regsiter the handler
    ///
    pub fn on_io<F>(&self, f: F) -> Subscription
    where
        F: Send + 'static + FnMut(IOKind, &str),
    {
        self.listener.on_io::<F>(f)
    }

    /// Get the server id
    pub fn server_id(&self) -> i32 {
        self.io.id()
    }

    /// Root Path of working project
    pub fn root_path(&self) -> &PathBuf {
        self.io.root_path()
    }

    /// Working dir of the workspace
    pub fn working_dir(&self) -> &PathBuf {
        self.io.working_dir()
    }

    /// List of capablilities
    pub fn capabilities(&self) -> ServerCapabilities {
        self.capabilities.read().clone()
    }

    /// Update the server capabilities
    pub fn update_capabilities(&self, update: impl FnOnce(&mut ServerCapabilities)) {
        update(self.capabilities.write().deref_mut())
    }

    /// List code action kinds
    pub fn code_action_kinds(&self) -> Option<Vec<CodeActionKind>> {
        self.code_action_kind.clone()
    }

    /// The server name
    pub fn name(&self) -> &str {
        self.io.name()
    }

    /// Kill the tasks
    pub fn kill(&mut self) -> anyhow::Result<()> {
        self.io.kill()?;
        self.listener.kill()?;
        Ok(())
    }
}
