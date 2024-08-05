use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Result};
use liblspc::{read_headers, LanguageServerBinary, LanguageSeverProcess, CONTENT_LEN_HEADER};
use serde::{Deserialize, Serialize};
use serde_json::{json, value::RawValue};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

#[tokio::main]
async fn main() -> Result<()> {
    let binary = LanguageServerBinary {
        path: PathBuf::from(OsString::from("rust-analyzer")),
        envs: None,
        args: Vec::new(),
    };
    let root = Path::new("/");

    let mut procc = LanguageSeverProcess::new(binary, root, None);
    let stdout = procc.process.lock().stdout.take().unwrap();

    let mut reader = BufReader::new(stdout);
    let mut buffer: Vec<u8> = Vec::new();
    {
        procc.initialize().await.unwrap()
    }

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

        if let Ok(AnyResponse {
            jsonrpc,
            id,
            error,
            result,
            ..
        }) = serde_json::from_slice(&buffer)
        {
            println!("{:?}", error);
            println!("{}", result.unwrap());
        }
    }
}
