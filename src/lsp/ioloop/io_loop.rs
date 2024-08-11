use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{anyhow, Context};
use log::warn;
use parking_lot::Mutex;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::{
    io::{AsyncReadExt, BufReader, BufWriter},
    process::{ChildStdin, ChildStdout},
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
};

use crate::types::types::LanguageServerBinary;
use crate::{handlers::input_handlers::read_headers, types::types::CONTENT_LEN_HEADER};

pub(crate) struct IoLoop {
    server: Arc<Mutex<Child>>,
    root_path: PathBuf,
    working_dir: PathBuf,
    response_tx: UnboundedSender<String>,
}

impl IoLoop {
    pub async fn new(
        binary: LanguageServerBinary,
        root_path: &Path,
        response_tx: UnboundedSender<String>,
        request_rx: UnboundedReceiver<String>,
    ) -> anyhow::Result<Self> {
        let working_dir = if root_path.is_dir() {
            root_path
        } else {
            root_path.parent().unwrap_or_else(|| Path::new("/"))
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

        let mut process = command.spawn().with_context(|| {
            format!(
                "failed to spawn command. path: {:?}, working directory: {:?}, args: {:?}",
                binary.path, working_dir, &binary.args
            )
        })?;

        let stdin = process.stdin.take().unwrap();
        let stdout = process.stdout.take().unwrap();

        Self::attach_stdin(stdin, request_rx).await?;
        Self::attach_stdout(stdout, response_tx.clone()).await?;

        Ok(Self {
            server: Arc::new(Mutex::new(process)),
            root_path: root_path.to_path_buf(),
            working_dir: root_path.to_path_buf(),
            response_tx: response_tx.clone(),
        })
    }

    async fn attach_stdin(
        stdin: ChildStdin,
        mut request_rx: UnboundedReceiver<String>,
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
        Ok(())
    }

    async fn attach_stdout(
        stdout: ChildStdout,
        response_tx: UnboundedSender<String>,
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
        }
    }
}
