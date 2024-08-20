use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::Result;
use liblspc::types::types::ProccessId;
use liblspc::{types::types::LanguageServerBinary, LanguageSeverProcess};
use lsp_types::{
    notification::{self, Initialized, Notification},
    request, InitializedParams, Registration, RegistrationParams, ShowMessageRequestParams,
};
use tokio::{io::join, join, task::yield_now};

#[tokio::main]
async fn main() -> Result<()> {
    let binary = LanguageServerBinary {
        path: PathBuf::from(OsString::from("rust-analyzer")),
        envs: None,
        args: Vec::new(),
    };
    let root = Path::new("/");

    let procc = LanguageSeverProcess::new(binary, root, ProccessId(0));
    let init_res = procc.initialize().await;
    println!("{:?}", init_res);
    let regis = RegistrationParams {
        registrations: [].to_vec(),
    };
    let inited_params = InitializedParams {};
    let inited = procc.notify::<Initialized>(inited_params).await;
    let registerd = procc.request::<request::RegisterCapability>(regis).await;
    println!("{:?}", registerd);
    Ok(())
}
