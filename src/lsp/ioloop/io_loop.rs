use anyhow::anyhow;
use log::warn;
use lsp_types::request::Request;
use parking_lot::Mutex;
use std::{any::Any, collections::HashMap, sync::Arc};
use tokio::{
    io::{AsyncReadExt, BufReader},
    process::{ChildStderr, ChildStdin, ChildStdout},
    sync::mpsc::UnboundedSender,
    task::JoinHandle,
};

use crate::{
    handlers::{self, input_handlers::read_headers},
    types::types::{
        AnyNotification, AnyResponse, IoHandler, IoKind, NotificationHandler, RequestId,
        ResponseHandler, CONTENT_LEN_HEADER,
    },
};

pub(crate) struct IoLoop {
    loop_handle: JoinHandle<anyhow::Result<()>>,
    io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
    response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
    notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
    notification_channel_tx: UnboundedSender<AnyNotification>,
}

impl IoLoop {
    fn new(
        stdin: ChildStdin,
        stdout: ChildStdout,
        stderr: ChildStderr,
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
        notification_channel_tx: UnboundedSender<AnyNotification>,
        request_in_rx: UnboundedSender<String>,
    ) -> Self {
        let loop_handle = tokio::spawn(Self::handle_stdout(
            stdout,
            io_handlers.clone(),
            response_handlers.clone(),
            notification_channel_tx.clone(),
        ));

        Self {
            loop_handle,
            io_handlers: io_handlers.clone(),
            response_handlers: response_handlers.clone(),
            notification_handlers: notification_handlers.clone(),
            notification_channel_tx: notification_channel_tx.clone(),
        }
    }

    async fn handle_stdin() -> anyhow::Result<()> {
        Ok(())
    }
    async fn handle_stdout(
        stdout: ChildStdout,
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        notification_channel_tx: UnboundedSender<AnyNotification>,
    ) -> anyhow::Result<()> {
        let mut buff_reader = BufReader::new(stdout);
        let mut buffer: Vec<u8> = Vec::new();

        loop {
            buffer.clear();
            read_headers(&mut buff_reader, &mut buffer);

            let header = std::str::from_utf8(&buffer)?;
            let message_len = header
                .split('\n')
                .find(|line| line.starts_with(CONTENT_LEN_HEADER))
                .and_then(|line| line.strip_prefix(CONTENT_LEN_HEADER))
                .ok_or_else(|| anyhow!("Invalid LSP message header: {:?}", header))?
                .trim_end()
                .parse()?;

            buffer.resize(message_len, 0);
            buff_reader.read_exact(&mut buffer).await?;

            if let Ok(message) = std::str::from_utf8(&buffer) {
                log::trace!("incoming message: {message}");
                for handler in io_handlers.lock().values_mut() {
                    handler(IoKind::StdOut, message);
                }
            }

            if let Ok(message) = serde_json::from_slice::<AnyNotification>(&buffer) {
                notification_channel_tx.send(message)?;
            } else if let Ok(AnyResponse {
                id, error, result, ..
            }) = serde_json::from_slice(&buffer)
            {
                let mut response_handlers = response_handlers.lock();
                if let Some(handler) = response_handlers
                    .as_mut()
                    .and_then(|handlers| handlers.remove(&id))
                {
                    drop(response_handlers);
                    if let Some(error) = error {
                        handler(Err(error));
                    } else if let Some(result) = result {
                        handler(Ok(result.get().into()));
                    } else {
                        handler(Ok("null".into()));
                    }
                }
            } else {
                warn!(
                    "Failed to deserialize LSP message: \n{}",
                    std::str::from_utf8(&buffer)?
                );
            }
        }
    }

    async fn handle_stderr(
        stderr: ChildStderr,
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
        request_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
        notification_channel_tx: UnboundedSender<AnyNotification>,
        request_in_rx: UnboundedSender<String>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
