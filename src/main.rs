use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Result};
use liblspc::handlers::input_handlers::read_headers;
use liblspc::{
    types::types::{AnyNotification, AnyResponse, LanguageServerBinary, CONTENT_LEN_HEADER},
    LanguageSeverProcess,
};
use tokio::io::{AsyncReadExt, BufReader};

#[tokio::main]
async fn main() -> Result<()> {
    let binary = LanguageServerBinary {
        path: PathBuf::from(OsString::from("clangd")),
        envs: None,
        args: Vec::new(),
    };
    let root = Path::new("/");

    let mut procc = LanguageSeverProcess::new(binary, root, None).unwrap();
    let stdout = procc.process.lock().stdout.take().unwrap();

    let mut reader = BufReader::new(stdout);
    let mut buffer: Vec<u8> = Vec::new();
    procc.initialize().await.unwrap();
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

        if let Ok(msg) = serde_json::from_slice::<AnyResponse>(&buffer) {
            println!("{:?}", msg);
        } else if let Ok(notification) = serde_json::from_slice::<AnyNotification>(&buffer) {
            println!("{:?}", notification);
        } else {
            println!("Invalid Response");
        }
    }
}
