use std::{collections::HashMap, sync::Arc};

use anyhow::anyhow;
use log::warn;
use parking_lot::Mutex;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::ChildStdout,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
};

use crate::types::types::{
    AnyNotification, AnyResponse, LspRequestId, NotificationHandler, ResponseHandler,
    CONTENT_LEN_HEADER, HEADER_DELIMITER,
};

pub struct LspChannelInputHandler {
    pub response_handlers: Arc<Mutex<Option<HashMap<LspRequestId, ResponseHandler>>>>,
    pub notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
    pub notification_channel: UnboundedReceiver<AnyNotification>,
    channel_sender: UnboundedSender<AnyNotification>,
}

impl LspChannelInputHandler {
    pub fn new(
        response_handlers: Arc<Mutex<Option<HashMap<LspRequestId, ResponseHandler>>>>,
        notification_handlers: Arc<Mutex<HashMap<&'static str, NotificationHandler>>>,
    ) -> Self {
        Self {
            response_handlers,
            notification_handlers,
        }
    }
    async fn handler(
        &mut self,
        stdout: ChildStdout,
        channel_sender: UnboundedSender<AnyNotification>,
    ) -> anyhow::Result<()> {
        let mut reader = BufReader::new(stdout);

        let mut buffer = Vec::new();

        loop {
            buffer.clear();
            read_headers(&mut reader, &mut buffer).await?;

            let headers = std::str::from_utf8(&buffer)?;

            let message_len: usize = headers
                .split('\n')
                .find(|line| line.starts_with(CONTENT_LEN_HEADER))
                .and_then(|line| line.strip_prefix(CONTENT_LEN_HEADER))
                .ok_or_else(|| anyhow!("Invalid lsp headers"))?
                .trim_end()
                .parse()?;

            buffer.resize(message_len, 0);

            reader.read_exact(&mut buffer).await?;

            if let Err(err) = std::str::from_utf8(&buffer) {
                return Err(anyhow!("Failed to get message {}", err));
            }

            if let Ok(msg) = serde_json::from_slice::<AnyNotification>(&buffer) {
                channel_sender.send(msg)?;
            } else if let Ok(AnyResponse {
                id, error, result, ..
            }) = serde_json::from_slice(&buffer)
            {
                let mut response_handlers = self.response_handlers.lock();

                if let Some(handler) = response_handlers
                    .as_mut()
                    .and_then(|handler| handler.remove(&id))
                {
                    drop(response_handlers);
                    if let Some(err) = error {
                        handler(Err(err));
                    } else if let Some(result) = result {
                        handler(Ok(result.get().into()));
                    } else {
                        handler(Ok("Null".into()));
                    }
                }
            } else {
                warn!(
                    "Failed to deserialize lsp message: \n{}",
                    std::str::from_utf8(&buffer)?
                );
            }
        }
    }
}

pub async fn read_headers(
    reader: &mut BufReader<ChildStdout>,
    buffer: &mut Vec<u8>,
) -> anyhow::Result<()> {
    loop {
        if buffer.len() >= HEADER_DELIMITER.len()
            && buffer[(buffer.len() - HEADER_DELIMITER.len())..] == HEADER_DELIMITER[..]
        {
            return Ok(());
        }
        if reader.read_until(b'\n', buffer).await? == 0 {
            return Err(anyhow!("cannot read headers from stdout"));
        }
    }
}
