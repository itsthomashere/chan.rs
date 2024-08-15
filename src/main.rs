use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::Result;
use liblspc::types::types::ProccessId;
use liblspc::{types::types::LanguageServerBinary, LanguageSeverProcess};

#[tokio::main]
async fn main() -> Result<()> {
    let binary = LanguageServerBinary {
        path: PathBuf::from(OsString::from("clangd")),
        envs: None,
        args: Vec::new(),
    };
    let root = Path::new("/");

    let procc = LanguageSeverProcess::new(binary, root, ProccessId(0));
    let init_res = procc.initialize().await;
    println!("{:?}", init_res);
    Ok(())
}
