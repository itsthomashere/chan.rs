use std::sync::atomic::AtomicI32;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::types::types::LspRequest;

pub(crate) struct Listener {
    next_id: AtomicI32,
    response_handlers: Arc<Mutex<Option<HashMap<LspRequestId, ResponseHandler>>>>,
    notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
    request_tx: UnboundedSender<String>,
}
