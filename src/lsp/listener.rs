use anyhow::{anyhow, Context};
use lsp_types::{notification, request};
use parking_lot::Mutex;
use serde_json::Value;
use std::future::{Future, IntoFuture};
use std::sync::Arc;
use std::{collections::HashMap, sync::atomic::AtomicI32};
use tokio::select;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use tokio::{sync::mpsc::UnboundedReceiver, task::JoinHandle};

use crate::types::{
    AnyResponse, IoKind, LspError, LspResult, Notification, Request, Response, Subscription,
    JSON_RPC_VER, LSP_REQUEST_TIMEOUT,
};
use crate::{
    types::{AnyNotification, IoHandler, NotificationHandler, RequestId, ResponseHandler},
    util,
};

pub(crate) struct Listener {
    next_id: AtomicI32,
    request_out_tx: UnboundedSender<String>,
    io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
    notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
    response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
    output_tasks: JoinHandle<anyhow::Result<()>>,
}

impl Listener {
    pub(crate) fn new(
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
        notification_channel_rx: UnboundedReceiver<AnyNotification>,
        request_out_tx: UnboundedSender<String>,
    ) -> anyhow::Result<Self> {
        let res_handler = response_handlers.clone();
        let noti_handler = notification_handlers.clone();
        let output_tasks = tokio::spawn(async move {
            Self::handle_output(noti_handler, res_handler, notification_channel_rx).await
        });

        Ok(Self {
            output_tasks,
            next_id: Default::default(),
            request_out_tx,
            io_handlers,
            response_handlers,
            notification_handlers,
        })
    }

    async fn handle_output(
        notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
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

        Ok(())
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
            .clone()
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

        let request_out_rx = &self.request_out_tx.clone();
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

    pub(crate) async fn send_notification<T: notification::Notification>(
        &self,
        params: T::Params,
    ) -> anyhow::Result<()> {
        let message = serde_json::to_string(&Notification {
            jsonrpc: JSON_RPC_VER,
            method: T::METHOD,
            params,
        })
        .unwrap();

        self.request_out_tx.send(message)?;

        Ok(())
    }

    pub(crate) fn on_request<T: request::Request, Fut, F>(&self, mut f: F) -> Subscription
    where
        Fut: 'static + Future<Output = anyhow::Result<T::Result>> + Send,
        F: 'static + Send + FnMut(T::Params) -> Fut + Send,
    {
        let request_out_tx = self.request_out_tx.clone();

        let prev_handler = self.notification_handlers.lock().insert(
            T::METHOD,
            Box::new(move |id, params| {
                if let Some(id) = id {
                    match serde_json::from_value::<T::Params>(params) {
                        Ok(params) => {
                            let response = f(params);

                            tokio::spawn({
                                let request_out_tx = request_out_tx.clone();
                                async move {
                                    let response = match response.await {
                                        Ok(result) => Response {
                                            jsonrpc: JSON_RPC_VER,
                                            id,
                                            value: LspResult::Ok(Some(result)),
                                        },
                                        Err(error) => Response {
                                            jsonrpc: JSON_RPC_VER,
                                            id,
                                            value: LspResult::Error(Some(LspError {
                                                message: error.to_string(),
                                            })),
                                        },
                                    };
                                    if let Ok(response) = serde_json::to_string(&response) {
                                        request_out_tx.send(response).ok();
                                    }
                                }
                            });
                        }
                        Err(error) => {
                            log::error!(
                                "Error deserializing {} lsp request: {:?}",
                                T::METHOD,
                                error
                            );
                            let response = AnyResponse {
                                jsonrpc: JSON_RPC_VER,
                                id,
                                error: Some(LspError {
                                    message: error.to_string(),
                                }),
                                result: None,
                            };
                            if let Ok(response) = serde_json::to_string(&response) {
                                request_out_tx.send(response).ok();
                            }
                        }
                    }
                }
            }),
        );

        assert!(
            prev_handler.is_none(),
            "registered multiple handlers for the same lsp method"
        );

        Subscription::Notification {
            method: T::METHOD,
            notification_handlers: Some(self.notification_handlers.clone()),
        }
    }

    pub(crate) fn on_io<F>(&self, f: F) -> Subscription
    where
        F: 'static + Send + FnMut(IoKind, &str),
    {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        self.io_handlers.lock().insert(id, Box::new(f));

        Subscription::Io {
            id,
            io_handlers: Some(Arc::downgrade(&self.io_handlers.clone())),
        }
    }

    pub(crate) fn on_notification<T: notification::Notification, F>(&self, mut f: F) -> Subscription
    where
        F: 'static + Send + FnMut(T::Params),
    {
        let prev_handler = self.notification_handlers.lock().insert(
            T::METHOD,
            Box::new(move |_, params| {
                if let Ok(params) = serde_json::from_value(params) {
                    f(params)
                }
            }),
        );
        assert!(
            prev_handler.is_none(),
            "Register multiple handle for the same notification method"
        );

        Subscription::Notification {
            method: T::METHOD,
            notification_handlers: Some(self.notification_handlers.clone()),
        }
    }

    pub(crate) fn remove_notification_handler<T: notification::Notification>(&self) {
        self.notification_handlers.lock().remove(T::METHOD);
    }

    pub(crate) fn remove_request_handler<T: request::Request>(&self) {
        self.notification_handlers.lock().remove(T::METHOD);
    }

    pub(crate) fn has_notification_handler<T: notification::Notification>(&self) -> bool {
        self.notification_handlers.lock().contains_key(T::METHOD)
    }

    pub(crate) fn has_request_handler<T: request::Request>(&self) -> bool {
        self.notification_handlers.lock().contains_key(T::METHOD)
    }

    pub(crate) fn kill(&self) -> anyhow::Result<()> {
        self.output_tasks.abort();
        drop(self.notification_handlers.lock());
        drop(self.response_handlers.lock());
        drop(self.io_handlers.lock());
        Ok(())
    }
}
