use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::{anyhow, Context};
use parking_lot::Mutex;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{self, ChildStderr};
use tokio::{
    io::{AsyncBufReadExt, BufReader, BufWriter},
    process::{Child, ChildStdin, ChildStdout},
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

use crate::process::LanguageServerBinary;
use crate::{
    utils, AnyNotification, AnyResponse, LSPError, RequestId, CONTENT_LEN_HEADER, HEADER_DELIMITER,
};

// Handler function of io tasks
pub(crate) type IoHandler = Box<dyn Send + FnMut(IOKind, &str)>;

// Handler function of request tasks
// Return response as string or LSP error
pub(crate) type ResponseHandler = Box<dyn Send + FnOnce(Result<String, LSPError>)>;

// Handler function of notification tasks
pub(crate) type NotificationHandler = Box<dyn Send + FnMut(Option<RequestId>, Value)>;

pub async fn read_header(
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
            return Err(anyhow!("Could not read header, not bytes read"));
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum IOKind {
    In,
    Out,
    Err,
}

pub(crate) struct IO {
    stderr_task: JoinHandle<anyhow::Result<()>>,
    stdin_task: JoinHandle<anyhow::Result<()>>,
    stdout_task: JoinHandle<anyhow::Result<()>>,
    process: Arc<Mutex<Child>>,
    working_dir: PathBuf,
    root_path: PathBuf,
    name: Arc<str>,
    id: i32,
}

impl IO {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: i32,
        binary: LanguageServerBinary,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
        request_rx: UnboundedReceiver<String>,
        notification_tx: UnboundedSender<AnyNotification>,
        output_done: UnboundedSender<String>,
        root_path: &Path,
        capture: Arc<Mutex<Option<String>>>,
    ) -> anyhow::Result<Self> {
        let working_dir = if root_path.is_dir() {
            root_path
        } else {
            root_path.parent().unwrap_or_else(|| Path::new("/"))
        };

        log::info!(
            "Starting LSP. Path: {:?}, working directory: {:?}, args: {:?}",
            binary.path.to_str(),
            working_dir.to_str(),
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

        let mut server = command
            .spawn()
            .with_context(|| "failed to spawn lsp server".to_string())?;

        let stdin = server.stdin.take().unwrap();
        let stdout = server.stdout.take().unwrap();
        let stderr = server.stderr.take().unwrap();

        let stderr_task = Self::stderr_task(stderr, io_handlers.clone(), capture);
        let stdout_task = Self::stdout_task(
            stdout,
            io_handlers.clone(),
            response_handlers.clone(),
            notification_tx,
        );

        let stdin_task = Self::stdin_task(
            stdin,
            response_handlers,
            io_handlers,
            request_rx,
            output_done,
        );

        let name = match binary.path.file_name() {
            Some(name) => name.to_string_lossy().into(),
            None => Arc::default(),
        };

        Ok(Self {
            stderr_task,
            stdin_task,
            stdout_task,
            process: Arc::new(Mutex::new(server)),
            working_dir: working_dir.to_path_buf(),
            root_path: root_path.to_path_buf(),
            name,
            id,
        })
    }

    pub fn stdin_task(
        stdin: ChildStdin,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
        mut request_rx: UnboundedReceiver<String>,
        output_done: UnboundedSender<String>,
    ) -> JoinHandle<anyhow::Result<()>> {
        tokio::spawn(async move {
            let mut buff_writer = BufWriter::new(stdin);
            let _clear_response_handler = utils::defer({
                let response_handlers = response_handlers.clone();
                move || {
                    response_handlers.lock().take();
                }
            });

            let mut content_len_buffer: Vec<u8> = Vec::new();

            while let Some(message) = request_rx.recv().await {
                log::trace!("IO got request: {}", message);

                for handler in io_handlers.lock().values_mut() {
                    handler(IOKind::In, &message);
                }

                content_len_buffer.clear();

                write!(content_len_buffer, "{}", message.len())?;
                buff_writer.write_all(CONTENT_LEN_HEADER.as_bytes()).await?;
                buff_writer.write_all(&content_len_buffer).await?;
                buff_writer.write_all(HEADER_DELIMITER).await?;
                buff_writer.write_all(message.as_bytes()).await?;
                buff_writer.flush().await?;
            }

            drop(output_done);

            Ok(())
        })
    }

    pub fn stdout_task(
        stdout: ChildStdout,
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
        response_handlers: Arc<Mutex<Option<HashMap<RequestId, ResponseHandler>>>>,
        notification_tx: UnboundedSender<AnyNotification>,
    ) -> JoinHandle<anyhow::Result<()>> {
        tokio::spawn(async move {
            let mut buff_reader = BufReader::new(stdout);
            let mut buffer: Vec<u8> = Vec::new();

            loop {
                buffer.clear();
                read_header(&mut buff_reader, &mut buffer).await?;

                let header = std::str::from_utf8(&buffer)?;

                let content_len = header
                    .split('\n')
                    .find(|line| line.starts_with(CONTENT_LEN_HEADER))
                    .and_then(|line| line.strip_prefix(CONTENT_LEN_HEADER))
                    .ok_or_else(|| anyhow!("Invalid LSP header"))?
                    .trim_end()
                    .parse()?;

                buffer.resize(content_len, 0);
                buff_reader.read_exact(&mut buffer).await?;

                // Check if message is valid utf8
                if let Ok(message) = std::str::from_utf8(&buffer) {
                    log::trace!("IO send : {}", message);
                    // We got response, execute the io handler
                    for handler in io_handlers.lock().values_mut() {
                        handler(IOKind::Out, message);
                    }
                }

                if let Ok(message) = serde_json::from_slice::<AnyNotification>(&buffer) {
                    notification_tx.send(message)?;
                } else if let Ok(AnyResponse {
                    id, result, error, ..
                }) = serde_json::from_slice(&buffer)
                {
                    let mut response_handlers = response_handlers.lock();

                    // Get the available handler method and execute it
                    if let Some(handler) = response_handlers
                        .as_mut()
                        .and_then(|handlers| handlers.remove(&id))
                    {
                        drop(response_handlers);

                        if let Some(error) = error {
                            handler(Err(error))
                        } else if let Some(result) = result {
                            handler(Ok(result.get().into()))
                        } else {
                            log::trace!("No result or error");
                            handler(Ok("null".into()))
                        }
                    }
                } else {
                    log::warn!(
                        "Failed to deserialize LSP message: {}",
                        std::str::from_utf8(&buffer)?
                    );
                }
            }
        })
    }

    pub fn stderr_task(
        stderr: ChildStderr,
        io_handlers: Arc<Mutex<HashMap<i32, IoHandler>>>,
        capture: Arc<Mutex<Option<String>>>,
    ) -> JoinHandle<anyhow::Result<()>> {
        tokio::spawn(async move {
            let mut buf_reader = BufReader::new(stderr);
            let mut buffer: Vec<u8> = Vec::new();

            loop {
                buffer.clear();
                let byte_read = buf_reader.read_until(b'\n', &mut buffer).await?;
                if byte_read == 0 {
                    return Ok(());
                }

                if let Ok(message) = std::str::from_utf8(&buffer) {
                    log::trace!("IO got error: {}", message);

                    for handler in io_handlers.lock().values_mut() {
                        handler(IOKind::Err, message);
                    }

                    if let Some(stderr) = capture.lock().as_mut() {
                        stderr.push_str(message)
                    }
                }
            }
        })
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

    pub(crate) fn id(&self) -> i32 {
        self.id
    }

    pub(crate) fn kill(&mut self) -> anyhow::Result<()> {
        self.stdin_task.abort();
        self.stdin_task.abort();
        self.stderr_task.abort();

        self.process.lock().start_kill()?;
        Ok(())
    }
}
