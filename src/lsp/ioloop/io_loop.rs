use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{anyhow, Context, Error};
use log::warn;
use parking_lot::Mutex;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::task::{yield_now, JoinHandle};
use tokio::{
    io::{AsyncReadExt, BufReader, BufWriter},
    process::{ChildStdin, ChildStdout},
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
};

use crate::types::types::{LanguageServerBinary, ProccessId};
use crate::{handlers::input_handlers::read_headers, types::types::CONTENT_LEN_HEADER};

pub(crate) struct IoLoop {
    server: Arc<Mutex<Child>>,
    server_id: ProccessId,
    name: Arc<str>,
    root_path: PathBuf,
    working_dir: PathBuf,
    io_task: Mutex<Option<(JoinHandle<Result<(), Error>>, JoinHandle<Result<(), Error>>)>>,
}

impl IoLoop {
    pub(crate) fn new(
        binary: LanguageServerBinary,
        server_id: ProccessId,
        root_path: &Path,
        response_tx: UnboundedSender<String>,
        request_rx: UnboundedReceiver<String>,
    ) -> Self {
        let working_dir = if root_path.is_dir() {
            root_path
        } else {
            root_path.parent().unwrap_or_else(|| Path::new("/"))
        };

        let name = match binary.path.file_name() {
            Some(name) => name.to_string_lossy().into(),
            None => Arc::default(),
        };
        let mut command = Command::new(&binary.path);
        command
            .current_dir(working_dir)
            .envs(binary.envs.unwrap_or_default())
            .args(&binary.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut process = command
            .spawn()
            .with_context(|| {
                format!(
                    "failed to spawn command. path: {:?}, working directory: {:?}, args: {:?}",
                    binary.path, working_dir, &binary.args
                )
            })
            .unwrap();

        let stdin = process.stdin.take().unwrap();
        let stdout = process.stdout.take().unwrap();

        let stdout_task = tokio::spawn(Self::attach_stdin(request_rx, stdin));
        let stdin_task = tokio::spawn(Self::attach_stdout(response_tx.clone(), stdout));

        Self {
            server: Arc::new(Mutex::new(process)),
            root_path: root_path.to_path_buf(),
            working_dir: root_path.to_path_buf(),
            io_task: Mutex::new(Some((stdin_task, stdout_task))),
            server_id,
            name,
        }
    }

    pub(crate) async fn attach_stdin(
        mut request_rx: UnboundedReceiver<String>,
        stdin: ChildStdin,
    ) -> anyhow::Result<()> {
        let mut buf_writer = BufWriter::new(stdin);
        let mut content_len_buffer = Vec::new();

        while let Some(req) = request_rx.recv().await {
            content_len_buffer.clear();

            write!(content_len_buffer, "{}", req.len()).unwrap();

            buf_writer.write_all(CONTENT_LEN_HEADER.as_bytes()).await?;
            buf_writer.write_all(&content_len_buffer).await?;
            buf_writer.write_all("\r\n\r\n".as_bytes()).await?;
            buf_writer.write_all(req.as_bytes()).await?;

            buf_writer.flush().await?;
        }
        yield_now().await;
        Ok(())
    }

    pub(crate) async fn attach_stdout(
        response_tx: UnboundedSender<String>,
        stdout: ChildStdout,
    ) -> anyhow::Result<()> {
        let mut buf_reader = BufReader::new(stdout);
        let mut buffer: Vec<u8> = Vec::new();

        loop {
            buffer.clear();
            read_headers(&mut buf_reader, &mut buffer).await?;

            let headers = match std::str::from_utf8(&buffer) {
                Ok(headers) => headers,
                Err(e) => {
                    warn!("Unable to check header: {}", e);
                    continue;
                }
            };

            let message_len: usize = headers
                .split('\n')
                .find(|line| line.starts_with(CONTENT_LEN_HEADER))
                .and_then(|line| line.strip_prefix(CONTENT_LEN_HEADER))
                .ok_or_else(|| anyhow!("Invalid headers"))?
                .trim_end()
                .parse()?;

            buffer.resize(message_len, 0);
            buf_reader.read_exact(&mut buffer).await?;

            if let Ok(msg) = std::str::from_utf8(&buffer) {
                let response_tx = response_tx.clone();
                response_tx.send(msg.to_string())?;
            } else {
                warn!("Failed to get message");
                continue;
            }
            yield_now().await;
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn server_id(&self) -> ProccessId {
        self.server_id
    }

    pub(crate) fn root_path(&self) -> &PathBuf {
        &self.root_path
    }

    pub(crate) fn working_dir(&self) -> &PathBuf {
        &self.working_dir
    }
}
