use std::{
    collections::HashMap,
    sync::{atomic::AtomicI32, Arc},
};

use anyhow::{anyhow, Context};
use log::warn;
use lsp_types::{notification, request};
use parking_lot::Mutex;
use tokio::sync::{
    mpsc::{UnboundedReceiver, UnboundedSender},
    oneshot,
};

use crate::types::types::{
    AnyNotification, AnyResponse, InternalLspRequest, LspRequest, LspRequestFuture, LspRequestId,
    NotificationHandler, ResponseHandler, JSONPRC_VER,
};

pub(crate) struct Listener {
    next_id: AtomicI32,
    response_handlers: Arc<Mutex<Option<HashMap<LspRequestId, ResponseHandler>>>>,
    notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
    request_tx: UnboundedSender<String>,
    output_tx: UnboundedSender<String>,
}

impl Listener {
    async fn new(
        response_rx: UnboundedReceiver<String>,
        request_tx: UnboundedSender<String>,
        output_tx: UnboundedSender<String>,
    ) -> anyhow::Result<Self> {
        let response_handlers = Arc::new(Mutex::new(Some(
            HashMap::<LspRequestId, ResponseHandler>::default(),
        )));
        let notification_handlers =
            Arc::new(Mutex::new(HashMap::<_, NotificationHandler>::default()));
        Self::response_listener(
            response_rx,
            response_handlers.clone(),
            notification_handlers.clone(),
        )
        .await?;
        Ok(Self {
            next_id: Default::default(),
            response_handlers,
            notification_handlers,
            request_tx,
            output_tx,
        })
    }

    async fn response_listener(
        mut response_rx: UnboundedReceiver<String>,
        response_handlers: Arc<Mutex<Option<HashMap<LspRequestId, ResponseHandler>>>>,
        notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
    ) -> anyhow::Result<()> {
        while let Some(msg) = response_rx.recv().await {
            if let Ok(AnyResponse {
                id, error, result, ..
            }) = serde_json::from_str(&msg)
            {
                let mut response_handlers = response_handlers.lock();
                if let Some(handler) = response_handlers
                    .as_mut()
                    .and_then(|handler| handler.remove(&id))
                {
                    drop(response_handlers);
                    if let Some(err) = error {
                        handler(Err(err));
                    } else if let Some(res) = result {
                        handler(Ok(res.get().into()));
                    } else {
                        handler(Ok("null".into()));
                    }
                };
            } else if let Ok(AnyNotification { id, method, params }) =
                serde_json::from_str::<AnyNotification>(&msg)
            {
                let mut notification_handlers = notification_handlers.lock();
                if let Some(mut handler) = notification_handlers.remove(method.as_str()) {
                    drop(notification_handlers);
                    if let Some(params) = params {
                        handler(id, params);
                    }
                }
            } else {
                warn!("Failed to deserialize lsp message:\n {}", msg);
            }
        }
        Ok(())
    }

    async fn send_request<T: request::Request>(
        &self,
        next_id: &AtomicI32,
        request_tx: UnboundedSender<String>,
        params: T::Params,
    ) -> impl LspRequestFuture<anyhow::Result<T::Result>>
    where
        T::Result: 'static + Send,
    {
        let request_tx = request_tx.clone();
        let id = next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let message = serde_json::to_string(&InternalLspRequest {
            jsonrpc: JSONPRC_VER,
            id: LspRequestId::Int(id),
            method: T::METHOD,
            params,
        })
        .unwrap();
        let (tx, mut rx) = oneshot::channel();
        let handle_response = self
            .response_handlers
            .lock()
            .as_mut()
            .ok_or_else(|| anyhow!("Server shutdown"))
            .map(|handler| {
                handler.insert(
                    LspRequestId::Int(id),
                    Box::new(move |result| {
                        tokio::spawn(async move {
                            let response = match result {
                                Ok(response) => match serde_json::from_str(&response) {
                                    Ok(deserialize) => Ok(deserialize),
                                    Err(err) => {
                                        log::error!(
                                            "Failed to deserialize response from io handler"
                                        );
                                        Err(err).context("Failed to deserialize message")
                                    }
                                },
                                Err(err) => Err(anyhow!("{}", err.message)),
                            };
                            _ = tx.send(response);
                        });
                    }),
                )
            });
        let send = request_tx
            .send(message)
            .context("Failed to write to language server stdin through the io loop");

        let _ = request_tx.downgrade();
        LspRequest::new(id, async move {
            handle_response.unwrap_or_default();
            send.unwrap_or_default();
            match rx.try_recv() {
                Ok(response) => response,
                Err(err) => Err(err.into()),
            }
        })
    }

    async fn send_notification<T: notification::Notification>() {}
}
