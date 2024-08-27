use anyhow::anyhow;
use log::warn;
use parking_lot::Mutex;
use std::io::Write;
use std::{collections::HashMap, sync::Arc};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::task::JoinHandle;
use tokio::{
    io::{AsyncReadExt, BufReader, BufWriter},
    process::{ChildStderr, ChildStdin, ChildStdout},
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
};

use crate::{
    handlers::input_handlers::read_headers,
    types::types::{
        AnyNotification, AnyResponse, IoHandler, IoKind, RequestId, ResponseHandler,
        CONTENT_LEN_HEADER,
    },
};

pub(crate) struct IoLoop {
    stdin_task: JoinHandle<anyhow::Result<()>>,
    stdout_task: JoinHandle<anyhow::Result<()>>,
    stderr_task: JoinHandle<anyhow::Result<()>>,
    notification_channel_tx: UnboundedSender<AnyNotification>,
}

impl IoLoop {
    fn new(
        stdin: ChildStdin,
        stdout: ChildStdout,
        stderr: ChildStderr,
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        notification_channel_tx: UnboundedSender<AnyNotification>,
        request_in_rx: UnboundedReceiver<String>,
        response_out_tx: UnboundedSender<String>,
        stderr_capture: Arc<Mutex<Option<String>>>,
    ) -> Self {
        let stdout_task = tokio::spawn(Self::handle_stdout(
            stdout,
            io_handlers.clone(),
            response_handlers.clone(),
            notification_channel_tx.clone(),
        ));

        let stdin_task = tokio::spawn(Self::handle_stdin(
            stdin,
            request_in_rx,
            response_out_tx,
            response_handlers.clone(),
            io_handlers.clone(),
        ));

        let stderr_task = tokio::spawn(Self::handle_stderr(stderr, io_handlers, stderr_capture));

        Self {
            stdin_task,
            stdout_task,
            stderr_task,
            notification_channel_tx: notification_channel_tx.clone(),
        }
    }

    async fn handle_stdin(
        stdin: ChildStdin,
        mut request_in_rx: UnboundedReceiver<String>,
        response_out_tx: UnboundedSender<String>,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
    ) -> anyhow::Result<()> {
        let mut buff_writer = BufWriter::new(stdin);
        let _clear_response_handlers = {
            let response_handlers = response_handlers.clone();
            move || {
                response_handlers.lock().take();
            }
        };

        let mut content_len_buffer = Vec::new();

        while let Some(message) = request_in_rx.recv().await {
            log::trace!("Incoming Lsp Request:{message}");

            for handler in io_handlers.lock().values_mut() {
                handler(IoKind::StdIn, &message);
            }

            content_len_buffer.clear();
            write!(content_len_buffer, "{}", message.len()).unwrap();
            buff_writer.write_all(CONTENT_LEN_HEADER.as_bytes()).await?;
            buff_writer.write_all(&content_len_buffer).await?;
            buff_writer.write_all("\r\n\r\n".as_bytes()).await?;
            buff_writer.write_all(message.as_bytes()).await?;
            buff_writer.flush().await?;
        }
        drop(response_out_tx);

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
        stderr_capture: Arc<Mutex<Option<String>>>,
    ) -> anyhow::Result<()> {
        let mut buff_reader = BufReader::new(stderr);
        let mut buffer: Vec<u8> = Vec::new();
        loop {
            buffer.clear();

            let byte_read = buff_reader.read_until(b'\n', &mut buffer).await?;
            if byte_read == 0 {
                return Ok(());
            }

            if let Ok(message) = std::str::from_utf8(&buffer) {
                log::trace!("Incoming Lsp Stderr message: {message}");
                for handler in io_handlers.lock().values_mut() {
                    handler(IoKind::StdErr, message);
                }

                if let Some(stderr) = stderr_capture.lock().as_mut() {
                    stderr.push_str(message);
                }
            };
            tokio::task::yield_now().await;
        }
    }
}
