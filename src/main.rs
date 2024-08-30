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
use lsp_types::{
    notification::Initialized,
    request::{Initialize, RegisterCapability},
    InitializeParams, InitializedParams, Registration, RegistrationParams,
};
use parking_lot::Mutex;

#[tokio::main]
async fn main() -> Result<()> {
    let binary = LanguageServerBinary {
        path: PathBuf::from(OsString::from("rust-analyzer")),
        envs: None,
        args: Vec::new(),
    };
    let root = Path::new("/");
    let stderr_capture: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(Some(String::default())));

    let procc =
        LanguageServerProcess::new(binary, ProccessId(0), root, stderr_capture.clone(), None)?;
    let init_params = InitializeParams::default();

    let response = procc.request::<Initialize>(init_params).await.unwrap();
    let response = serde_json::to_string_pretty(&response).unwrap();

    println!("{}\n", response);

    let inited = InitializedParams {};
    let _ = procc.notify::<Initialized>(inited).await;
    let regis = RegistrationParams {
        registrations: vec![Registration {
            id: "testing_hehe".to_string(),
            method: "text/willSaveWaitUntil".to_string(),
            register_options: None,
        }],
    };
    let registerd = procc.request::<RegisterCapability>(regis).await;
    println!("{:?}\n", registerd);
    println!("{:?}\n", stderr_capture.lock().as_slice());

    Ok(())
}
