use anyhow::{anyhow, Context};
use lsp_types::request;
use parking_lot::Mutex;
use serde_json::Value;
use std::future::IntoFuture;
use std::sync::Arc;
use std::{collections::HashMap, sync::atomic::AtomicI32};
use tokio::select;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use tokio::{sync::mpsc::UnboundedReceiver, task::JoinHandle};

use crate::types::types::{Request, JSON_RPC_VER, LSP_REQUEST_TIMEOUT};
use crate::{
    ioloop::io_loop::IoLoop,
    types::types::{AnyNotification, IoHandler, NotificationHandler, RequestId, ResponseHandler},
    util::util,
};
pub(crate) struct Listener {
    next_id: AtomicI32,
    request_out_rx: UnboundedSender<String>,
    io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
    response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
    notification_channel_rx: UnboundedReceiver<AnyNotification>,
    output_tasks: JoinHandle<anyhow::Result<()>>,
}

impl Listener {
    pub(crate) fn new(
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
        notification_channel_rx: UnboundedReceiver<AnyNotification>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            io_handlers: todo!(),
            response_handlers: todo!(),
            notification_channel_rx: todo!(),
        })
    }

    async fn handle_output(
        notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        stdin_task: JoinHandle<anyhow::Result<()>>,
        mut notification_channel_rx: UnboundedReceiver<AnyNotification>,
    ) -> anyhow::Result<()> {
        let _clear_response_handlers = util::defer({
            let response_handlers = response_handlers.clone();
            move || {
                response_handlers.lock().take();
            }
        });

        while let Some(message) = notification_channel_rx.recv().await {
            {
                let mut notification_handlers = notification_handlers.lock();
                if let Some(handler) = notification_handlers.get_mut(message.method.as_str()) {
                    handler(message.id, message.params.unwrap_or(Value::Null));
                } else {
                    drop(notification_handlers);
                }
            }

            tokio::task::yield_now().await;
        }

        stdin_task.await?
    }

    pub(crate) async fn request<T: request::Request>(
        &self,
        params: T::Params,
    ) -> anyhow::Result<T::Result> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let message = serde_json::to_string(&Request {
            jsonrpc: JSON_RPC_VER,
            id: RequestId::Int(id),
            method: T::METHOD,
            params,
        })
        .unwrap();

        let (tx, rx) = oneshot::channel();

        let handle_response = self
            .response_handlers
            .lock()
            .as_mut()
            .ok_or_else(|| anyhow!("Server shutdown"))
            .map(|handler| {
                handler.insert(
                    RequestId::Int(id),
                    Box::new(move |result| {
                        tokio::spawn(async move {
                            let response = match result {
                                Ok(message) => match serde_json::from_str(&message) {
                                    Ok(desirialized) => Ok(desirialized),
                                    Err(error) => {
                                        log::error!("Failed to deserialize the LSP response: {}. Error: {}", message, error);
                                        Err(error).context("Failed to deserialize LSP message")
                                    }
                                },
                                Err(e) => Err(anyhow!("{}", e.message)),
                            };
                            _ =tx.send(response)
                        });
                    }),
                )
            });

        let request_out_rx = &self.request_out_rx.clone();
        let send = request_out_rx
            .send(message)
            .context("Failed to write to LSP stdin");
        let _ = request_out_rx.downgrade();

        let timeout_task = tokio::spawn(async move {
            tokio::time::sleep(LSP_REQUEST_TIMEOUT).await;
        });

        let response_handle = tokio::spawn(async move {
            handle_response.unwrap_or_default();
            send.unwrap_or_default();
            match rx.into_future().await {
                Ok(response) => response,
                Err(e) => Err(e.into()),
            }
        });

        select! {
            response = response_handle => {
                match response  {
                    Ok(res) => res,
                    Err(e)=> Err(e.into())
                }
            }
            _ = timeout_task => {
                    anyhow::bail!("Lsp Request time out");
                }
        }
    }
}
