use std::{io::Stdout, path::Path, process::Stdio};

use tokio::process;

mod lsp;

#[tokio::main]
async fn main() {
    initialize_lsp_fake().await;
}

async fn initialize_lsp_fake() {
    let working_dir = Path::new("/home/dacbui308/Projects/rust/liblspc/test-cproject/");
    let mut command = process::Command::new(
        "/home/dacbui308/Projects/rust/liblspc/lsp-bin/clangd_18.1.3/bin/clangd",
    );

    let output = command
        .current_dir(working_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .unwrap();
    println!("{:?}", output)
}
