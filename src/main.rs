use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::anyhow;
use liblspc::{LanguageServerBinary, LanguageSeverProcess, CONTENT_LEN_HEADER};
use lsp_types::{InitializeResult, ServerInfo};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

#[tokio::main]
async fn main() {
    let binary = LanguageServerBinary {
        path: PathBuf::from(OsString::from("rust-analyzer")),
        envs: None,
        args: Vec::new(),
    };
    let root = Path::new("/");

    let mut procc = LanguageSeverProcess::new(binary, root);
    let stdout = procc.process.lock().stdout.take().unwrap();
    {
        procc.initialize().await.unwrap()
    }

    let mut reader = BufReader::new(stdout);
    let mut buffer: Vec<u8> = Vec::new();
    loop {
        if let Ok(message) = reader.read_until(b'\n', &mut buffer).await {
            let headers = std::str::from_utf8(&buffer).unwrap_or_default();

            let message_len = headers
                .split('\n')
                .find(|line| line.starts_with(CONTENT_LEN_HEADER))
                .and_then(|line| line.strip_prefix(CONTENT_LEN_HEADER))
                .ok_or_else(|| anyhow!("Failed to parse headerse"))
                .unwrap()
                .trim_end()
                .parse()
                .unwrap();

            buffer.resize(message_len, 0);

            reader.read_exact(&mut buffer).await.unwrap();
            let buf2 = buffer.to_vec();
            println!("{}", std::str::from_utf8(&buffer).unwrap());
            let message_json: Response<InitializeResult> = serde_json::from_slice(&buf2).unwrap();
            println!("{:?}", message_json)
        };
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Response<'a, T> {
    jsonrpc: &'a str,
    id: i32,
    #[serde(flatten)]
    value: LspResult<T>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LspResult<T> {
    #[serde(rename = "result")]
    Ok(Option<T>),
    Error(Option<Error>),
}

#[derive(Debug, Serialize, Deserialize)]
struct Error {
    message: String,
}
