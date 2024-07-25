use std::process::Stdio;

use tokio::{io::AsyncReadExt, process};

mod lsp;

#[tokio::main]
async fn main() {
    let mut binding = process::Command::new("clangd");
    let command = binding
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command.spawn().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut string = String::new();
    stdout.read_to_string(&mut string).await.unwrap();

    println!("{}", string)
}
