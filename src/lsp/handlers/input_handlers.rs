use anyhow::anyhow;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::ChildStdout,
};

use crate::types::types::HEADER_DELIMITER;

pub async fn read_headers(
    reader: &mut BufReader<ChildStdout>,
    buffer: &mut Vec<u8>,
) -> anyhow::Result<()> {
    loop {
        if buffer.len() >= HEADER_DELIMITER.len()
            && buffer[(buffer.len() - HEADER_DELIMITER.len())..] == HEADER_DELIMITER[..]
        {
            return Ok(());
        }
        if reader.read_until(b'\n', buffer).await? == 0 {
            return Err(anyhow!("cannot read headers from stdout"));
        }
    }
}
