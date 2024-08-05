use std::io::Write;
use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};

use anyhow::{anyhow, Ok};
use lsp_types::{
    request::{self, Request},
    InitializeParams,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::ChildStdout;
use tokio::{
    io::{AsyncWriteExt, BufWriter},
    process::{Child, Command},
};

pub const CONTENT_LEN_HEADER: &str = "Content-Length: ";
pub const JSONPRC_VER: &str = "2.0";
const HEADER_DELIMITER: &[u8; 4] = b"\r\n\r\n";
pub struct LanguageServerBinary {
    pub path: PathBuf,
    pub envs: Option<HashMap<String, String>>,
    pub args: Vec<OsString>,
}

pub struct LanguageSeverProcess {
    pub process: Arc<Mutex<Child>>,
}

impl LanguageSeverProcess {
    pub fn new(binary: LanguageServerBinary, root_path: &Path) -> Self {
        let root_dir = if root_path.is_dir() {
            root_path
        } else {
            Path::new("/")
        };

        let mut command = Command::new(&binary.path);
        command
            .current_dir(root_dir)
            .args(&binary.args)
            .envs(binary.envs.unwrap_or_default())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let server = command.spawn().unwrap();

        Self {
            process: Arc::new(Mutex::new(server)),
        }
    }

    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        let mut server = self.process.lock();
        let stdin = server.stdin.take().unwrap();

        let params = InitializeParams::default();
        let message = serde_json::to_string(&LspRequest {
            jsonprc: JSONPRC_VER,
            id: 0,
            method: request::Initialize::METHOD,
            params,
        })
        .unwrap();
        {
            let mut content_len_buffer = Vec::new();
            write!(content_len_buffer, "{}", message.as_bytes().len()).unwrap();

            let mut bufwriter = BufWriter::new(stdin);

            bufwriter.write_all(CONTENT_LEN_HEADER.as_bytes()).await?;
            bufwriter.write_all(&content_len_buffer).await?;
            bufwriter.write_all("\r\n\r\n".as_bytes()).await?;
            bufwriter.write_all(message.as_bytes()).await?;
            bufwriter.flush().await?;
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct LspRequest<'a, T> {
    jsonprc: &'a str,
    id: i32,
    method: &'a str,
    params: T,
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
