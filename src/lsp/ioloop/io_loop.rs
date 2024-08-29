use anyhow::{anyhow, Context};
use log::warn;
use parking_lot::Mutex;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::{collections::HashMap, sync::Arc};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::process::{self, Child};
use tokio::task::JoinHandle;
use tokio::{
    io::{AsyncReadExt, BufReader, BufWriter},
    process::{ChildStderr, ChildStdin, ChildStdout},
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
};

use crate::types::types::{LanguageServerBinary, ProccessId};
use crate::util::util;
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
    working_dir: PathBuf,
    root_path: PathBuf,
    name: Arc<str>,
    server_id: ProccessId,
    server: Arc<Mutex<Option<Child>>>,
}

impl IoLoop {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        server_id: ProccessId,
        binary: LanguageServerBinary,
        root_path: &Path,
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        notification_channel_tx: UnboundedSender<AnyNotification>,
        request_in_rx: UnboundedReceiver<String>,
        output_done_tx: UnboundedSender<String>,
        stderr_capture: Arc<Mutex<Option<String>>>,
    ) -> anyhow::Result<Self> {
        let working_dir = if root_path.is_dir() {
            root_path
        } else {
            root_path.parent().unwrap_or_else(|| Path::new("/"))
        };

        log::info!(
            "Starting LSP. Bin Path: {:?}, working directory: {:?}, arguments: {:?}",
            binary.path,
            working_dir,
            &binary.args
        );

        let mut command = process::Command::new(&binary.path);
        command
            .current_dir(working_dir)
            .args(&binary.args)
            .envs(binary.envs.unwrap_or_default())
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut server = command.spawn().with_context(|| {
            format!(
                "Failed to spawn lsp. Path: {:?}. Working directory: {:?}. Arguments: {:?}",
                binary.path, working_dir, &binary.args
            )
        })?;

        let stdin = server.stdin.take().unwrap();
        let stdout = server.stdout.take().unwrap();
        let stderr = server.stderr.take().unwrap();

        // Clone the arcs to send to move other threads
        let ih_1 = io_handlers.clone();
        let ih_2 = io_handlers.clone();
        let res_handler = response_handlers.clone();
        let noti_channel = notification_channel_tx.clone();

        let stdout_task = tokio::spawn(async move {
            Self::handle_stdout(stdout, ih_1, response_handlers.clone(), noti_channel).await
        });

        let stdin_task = tokio::spawn(async move {
            Self::handle_stdin(stdin, request_in_rx, output_done_tx, res_handler, ih_2).await
        });

        let stderr_task =
            tokio::spawn(
                async move { Self::handle_stderr(stderr, io_handlers, stderr_capture).await },
            );
        let name: Arc<str> = match binary.path.file_name() {
            Some(name) => name.to_string_lossy().into(),
            None => Arc::default(),
        };
        Ok(Self {
            server: Arc::new(Mutex::new(Some(server))),
            server_id,
            name,
            root_path: root_path.to_path_buf(),
            working_dir: working_dir.to_path_buf(),
            stdin_task,
            stdout_task,
            stderr_task,
            notification_channel_tx: notification_channel_tx.clone(),
        })
    }

    async fn handle_stdin(
        stdin: ChildStdin,
        mut request_in_rx: UnboundedReceiver<String>,
        output_done_tx: UnboundedSender<String>,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
    ) -> anyhow::Result<()> {
        let mut buff_writer = BufWriter::new(stdin);
        let _clear_response_handlers = util::defer({
            let response_handlers = response_handlers.clone();
            move || {
                response_handlers.lock().take();
            }
        });

        let mut content_len_buffer = Vec::new();

        while let Some(message) = request_in_rx.recv().await {
            println!("StdIn got request: {message}");
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
        drop(output_done_tx);

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
            read_headers(&mut buff_reader, &mut buffer).await?;

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
                println!("StdOut Got response: {message}");
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
                println!("StdErr: Got: {message}");
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

    pub(crate) fn working_dir(&self) -> &PathBuf {
        &self.working_dir
    }

    pub(crate) fn root_path(&self) -> &PathBuf {
        &self.root_path
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn server_id(&self) -> ProccessId {
        self.server_id
    }

    pub(crate) fn kill(&self) -> anyhow::Result<()> {
        self.stdin_task.abort();
        self.stdout_task.abort();
        self.stderr_task.abort();
        self.server.lock().take().unwrap().start_kill()?;
        let _ = self.notification_channel_tx.downgrade();

        Ok(())
    }
}
