use std::sync::Arc;

use lsp_types::ClientCapabilities;

use crate::{types::types::ProccessId, LanguageServerProcess};

pub struct LanguageServiceClient {
    server_id: Option<ProccessId>,
    server: Arc<LanguageServerProcess>,
    client_capabilities: ClientCapabilities,
}

impl LanguageServiceClient {
    pub fn new(
        mut server: Arc<LanguageServerProcess>,
        client_capabilities: ClientCapabilities,
    ) -> anyhow::Result<Self> {
        // Find server in the server pool
        server = server.clone();
        //Initialize
        //Notify initilized
        //Register client capabilities
        Ok(Self {
            server_id: Some(server.server_id()),
            server,
            client_capabilities,
        })
    }
}
