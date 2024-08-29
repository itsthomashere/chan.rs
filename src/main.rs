use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Result;
use liblspc::{
    types::types::{LanguageServerBinary, ProccessId},
    LanguageServerProcess,
};
use lsp_types::{request::Initialize, InitializeParams};

#[tokio::main]
async fn main() -> Result<()> {
    let binary = LanguageServerBinary {
        path: PathBuf::from(OsString::from("rust-analyzer")),
        envs: None,
        args: Vec::new(),
    };
    let root = Path::new("/");

    let procc = LanguageServerProcess::new(binary, ProccessId(0), root, Arc::default(), None)?;
    let init_params = InitializeParams::default();

    let response = procc.request::<Initialize>(init_params).await;

    println!("{:?}", response);

    Ok(())
}
