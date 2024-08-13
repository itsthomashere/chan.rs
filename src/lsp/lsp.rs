pub mod handlers;
pub mod ioloop;
pub mod listener;
pub mod types;

use std::collections::HashMap;
use std::sync::atomic::AtomicI32;
use std::sync::Arc;

use ioloop::io_loop::IoLoop;
use listener::listener::Listener;
use lsp_types::{CodeActionKind, ServerCapabilities};
use parking_lot::{Mutex, RwLock};
use tokio::process::Child;
use types::types::{LspRequestId, NotificationHandler, ResponseHandler};

pub struct LanguageSeverProcess {
    name: Arc<str>,
    pub process: Arc<Mutex<Child>>,
    next_id: AtomicI32,
    capabilities: RwLock<ServerCapabilities>,
    code_action_kinds: Option<Vec<CodeActionKind>>,
    response_handlers: Arc<Mutex<Option<HashMap<LspRequestId, ResponseHandler>>>>,
    notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
    io_loop: Arc<IoLoop>,
    listener: Arc<Listener>,
}
impl LanguageSeverProcess {}
