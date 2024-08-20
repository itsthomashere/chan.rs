use std::{
    collections::HashMap,
    future::IntoFuture,
    sync::{atomic::AtomicI32, Arc},
};

use anyhow::{anyhow, Context};
use log::warn;
use lsp_types::{notification, request};
use parking_lot::Mutex;
use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    select,
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    task::{yield_now, JoinHandle},
};

use crate::types::types::{
    AnyNotification, AnyResponse, Error, InternalLspRequest, LspNotification, LspRequestId,
    LspResponse, LspResult, NotificationHandler, ResponseHandler, Subscription, JSONPRC_VER,
    LSP_REQUEST_TIMEOUT,
};

pub(crate) struct Listener {
    next_id: AtomicI32,
    response_handlers: Arc<Mutex<Option<HashMap<LspRequestId, ResponseHandler>>>>,
    notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
    request_tx: UnboundedSender<String>,
    response_task: Mutex<JoinHandle<Result<(), anyhow::Error>>>,
    output_tx: UnboundedSender<String>,
}

impl Listener {
    pub(crate) fn new(
        response_rx: UnboundedReceiver<String>,
        request_tx: UnboundedSender<String>,
        output_tx: UnboundedSender<String>,
    ) -> Self {
        let response_handlers = Arc::new(Mutex::new(Some(
            HashMap::<LspRequestId, ResponseHandler>::default(),
        )));
        let notification_handlers =
            Arc::new(Mutex::new(HashMap::<_, NotificationHandler>::default()));

        let response_task = tokio::spawn({
            let response_handlers = response_handlers.clone();
            let notification_handlers = notification_handlers.clone();
            Self::response_listener(response_handlers, notification_handlers, response_rx)
        });

        Self {
            next_id: Default::default(),
            response_handlers,
            notification_handlers,
            request_tx,
            output_tx,
            response_task: Mutex::new(response_task),
        }
    }

    pub(crate) async fn response_listener(
        response_handlers: Arc<Mutex<Option<HashMap<LspRequestId, ResponseHandler>>>>,
        notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
        mut response_rx: UnboundedReceiver<String>,
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
            yield_now().await;
        }
        Ok(())
    }

    pub(crate) async fn send_request<T: request::Request>(
        &self,
        params: T::Params,
    ) -> anyhow::Result<T::Result>
    where
        T::Result: 'static + Send,
    {
        let request_tx = self.request_tx.clone();
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let message = serde_json::to_string(&InternalLspRequest {
            jsonrpc: JSONPRC_VER,
            id: LspRequestId::Int(id),
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
                                Err(err) => Err(anyhow!("{:?}", err)),
                            };
                            let _ = tx.send(response);
                        });
                    }),
                );
            });
        yield_now().await;
        let send = request_tx
            .send(message)
            .context("Failed to write to language server stdin through the io loop");

        let _ = request_tx.downgrade();

        let response_handle = tokio::spawn(async move {
            handle_response.unwrap_or_default();
            send.unwrap_or_default();
            match rx.into_future().await {
                Ok(response) => response,
                Err(e) => Err(e.into()),
            }
        });
        let time_out = tokio::spawn(async move {
            tokio::time::sleep(LSP_REQUEST_TIMEOUT).await;
        });

        select! {
            response = response_handle => {
                match response {
                    Ok(res) => res,
                    Err(e) => Err(e.into()),
                }
            }
            _ = time_out => {
                anyhow::bail!("Lsp request timed out");
            }
        }
    }

    // [TODO): Make this return the notification into output_tx
    pub(crate) async fn send_notification<T: notification::Notification>(
        &self,
        params: T::Params,
    ) -> anyhow::Result<()> {
        let request_tx = self.request_tx.clone();
        let message = serde_json::to_string(&LspNotification {
            jsonrpc: JSONPRC_VER,
            method: T::METHOD,
            params,
        })
        .unwrap();

        let notify_task = tokio::spawn(async move {
            request_tx
                .send(message)
                .context("Failed to write to lsp stdin")
                .unwrap_or_default();
        });

        let time_out = tokio::spawn(async move {
            tokio::time::sleep(LSP_REQUEST_TIMEOUT).await;
        });
        select! {
            _ = notify_task => {}
            _ = time_out => {
                anyhow::bail!("Lsp request timed out");
            }
        }

        Ok(())
    }

    pub(crate) fn on_notification<F, Params>(&self, method: &'static str, mut f: F) -> Subscription
    where
        F: 'static + FnMut(Params) + Send,
        Params: DeserializeOwned,
    {
        let previous_handler = self.notification_handlers.lock().insert(
            method,
            Box::new(move |_, params| {
                if let Ok(params) = serde_json::from_value(params) {
                    f(params);
                };
            }),
        );

        assert!(
            previous_handler.is_none(),
            "Registered multiple hanlers for the same methods"
        );

        Subscription::Notification {
            method,
            notification_handlers: Some(self.notification_handlers.clone()),
        }
    }

    pub(crate) fn on_request<F, Res, Params>(&self, method: &'static str, mut f: F) -> Subscription
    where
        F: 'static + FnMut(Params) -> anyhow::Result<Res> + Send,
        Params: DeserializeOwned + Send + 'static,
        Res: 'static + Serialize + Send,
    {
        let output_tx = self.output_tx.clone();
        let previous_handler = self.notification_handlers.lock().insert(
            method,
            Box::new(move |id, params| {
                println!("id: {:?}. Params: {:?}", id, params);
                if let Some(id) = id {
                    match serde_json::from_value::<Params>(params) {
                        Ok(params) => {
                            let response = f(params);
                            tokio::spawn({
                                let output_tx = output_tx.clone();
                                async move {
                                    let response = match response {
                                        Ok(result) => LspResponse {
                                            jsonrpc: JSONPRC_VER,
                                            id,
                                            value: LspResult::Ok(Some(result)),
                                        },
                                        Err(error) => LspResponse {
                                            jsonrpc: JSONPRC_VER,
                                            id,
                                            value: LspResult::Error(Some(Error {
                                                message: error.to_string(),
                                            })),
                                        },
                                    };
                                    if let Ok(response) = serde_json::to_string(&response) {
                                        println!("{}", response);
                                        output_tx.send(response).ok();
                                    }
                                    yield_now().await;
                                }
                            });
                        }
                        Err(error) => {
                            log::error!("error deserializing {} request {:?}", method, error);
                            let response = AnyResponse {
                                jsonrpc: JSONPRC_VER,
                                id,
                                result: None,
                                error: Some(Error {
                                    message: error.to_string(),
                                }),
                            };
                            if let Ok(response) = serde_json::to_string(&response) {
                                output_tx.send(response).ok();
                            }
                        }
                    }
                } else {
                    println!("Failed");
                }
            }),
        );

        assert!(
            previous_handler.is_none(),
            "Registered multiple hanlers for the same methods"
        );

        Subscription::Notification {
            method,
            notification_handlers: Some(self.notification_handlers.clone()),
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.response_task.lock().abort();
        let _ = self.output_tx.downgrade();
    }
}
