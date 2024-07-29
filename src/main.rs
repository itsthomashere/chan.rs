use std::path::Path;

use lsp::lsp::{LanguageServer, LanguageServerBinary};

mod lsp;

#[tokio::main]
async fn main() {
    let lspbin = LanguageServerBinary {
        path: Path::new("/home/dacbui308/Projects/rust/liblspc/lsp-bin/clangd_18.1.3/bin/clangd")
            .to_path_buf(),
        envs: None,
        args: Vec::new(),
    };

    let root_path = Path::new("/home/dacbui308/Projects/rust/liblspc/");
    let server_id = lsp::lsp::LspId(9);

    let code_action_kinds = None;

    let new_server = LanguageServer::new(lspbin, server_id, root_path, code_action_kinds).unwrap();
    println!("{:?}", new_server)
}
