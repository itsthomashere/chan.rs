use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::Result;
use liblspc::types::types::ProccessId;
use liblspc::{types::types::LanguageServerBinary, LanguageSeverProcess};
use lsp_types::{
    notification::{Initialized, ShowMessage},
    request, InitializeParams, InitializedParams, Registration, RegistrationParams,
};

#[tokio::main]
async fn main() -> Result<()> {
    let binary = LanguageServerBinary {
        path: PathBuf::from(OsString::from("rust-analyzer")),
        envs: None,
        args: Vec::new(),
    };
    let root = Path::new("/");

    let procc = LanguageSeverProcess::new(binary, root, ProccessId(0));
    procc.initialize(InitializeParams::default()).await?;
    println!("working dir: {:?}", procc.working_dir());
    println!("root path : {:?}", procc.root_path());
    let regis = RegistrationParams {
        registrations: vec![Registration {
            id: "testing_hehe".to_string(),
            method: "text/willSaveWaitUntil".to_string(),
            register_options: None,
        }],
    };
    let inited_params = InitializedParams {};
    procc.on_notification::<ShowMessage, _>(move |params| {
        println!("Got notification: {:?}\n", params);
    })?;
    procc.notify::<Initialized>(inited_params).await?;
    let registerd = procc.request::<request::RegisterCapability>(regis).await;
    println!("{:?}", registerd);
    Ok(())
}
