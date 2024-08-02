use crate::{
    AnyNotification, AnyResponse, IoHandler, IoKind, RequestId, ResponseHandler,
    CONTENT_LENGTH_HEADERS,
};
use std::{collections::HashMap, future::Future, sync::Arc};

use anyhow::{anyhow, Result};
use log::warn;
use parking_lot::Mutex;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::ChildStdout,
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
};

const HEADER_DELIMITER: &[u8; 4] = b"\r\n\r\n";

pub struct LspStdoutHandler {
    pub(super) loop_handle: Box<dyn Future<Output = Result<()>>>,
    pub(super) notification_channel: UnboundedReceiver<AnyNotification>,
}

impl LspStdoutHandler {
    pub fn new(
        stdout: ChildStdout,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        io_handers: Arc<Mutex<HashMap<i32, IoHandler>>>,
    ) -> Self {
        let (tx, notification_channel) = mpsc::unbounded_channel();

        let loop_handle = Box::new(Self::handle(stdout, tx, response_handlers, io_handers));
        Self {
            loop_handle,
            notification_channel,
        }
    }

    pub async fn handle(
        stdout: ChildStdout,
        notification_sender: UnboundedSender<AnyNotification>,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        io_handers: Arc<Mutex<HashMap<i32, IoHandler>>>,
    ) -> anyhow::Result<()> {
        let mut stdout = BufReader::new(stdout);

        let mut buffer = Vec::new();
        loop {
            buffer.clear();

            read_headers(&mut stdout, &mut buffer).await?;
            let headers = std::str::from_utf8(&buffer)?;

            let message_len = headers
                .split('\n')
                .find(|line| line.starts_with(CONTENT_LENGTH_HEADERS))
                .and_then(|line| line.strip_prefix(CONTENT_LENGTH_HEADERS))
                .ok_or_else(|| anyhow!("Invalid message header {:?}", headers))?
                .trim_end()
                .parse()?;

            buffer.resize(message_len, 0);
            stdout.read_exact(&mut buffer).await?;

            if let Ok(message) = std::str::from_utf8(&buffer) {
                for handle in io_handers.lock().values_mut() {
                    handle(IoKind::StdOut, message)
                }
            }

            if let Ok(mesg) = serde_json::from_slice::<AnyNotification>(&buffer) {
                notification_sender.send(mesg)?;
            } else if let Ok(AnyResponse {
                id, error, result, ..
            }) = serde_json::from_slice(&buffer)
            {
                let mut response_handler = response_handlers.lock();
                if let Some(handler) = response_handler
                    .as_mut()
                    .and_then(|handler| handler.remove(&id))
                {
                    drop(response_handler);
                    if let Some(error) = error {
                        handler(Err(error))
                    } else if let Some(result) = result {
                        handler(Ok(result.get().into()));
                    } else {
                        handler(Ok("Null".into()));
                    }
                };
            } else {
                warn!(
                    "Failed to deserialize message from lsp: \n{}",
                    std::str::from_utf8(&buffer)?
                );
            }
        }
    }
}

pub(self) async fn read_headers(
    reader: &mut BufReader<ChildStdout>,
    buffer: &mut Vec<u8>,
) -> Result<()> {
    loop {
        if buffer.len() >= HEADER_DELIMITER.len()
            && buffer[(buffer.len() - HEADER_DELIMITER.len())..] == HEADER_DELIMITER[..]
        {
            return Ok(());
        }

        if reader.read_until(b'\n', buffer).await? == 0 {
            return Err(anyhow!("Cannot read Lsp headers"));
        }
    }
}
