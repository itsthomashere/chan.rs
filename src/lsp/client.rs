use std::{
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

use log::log;
use lsp_types::*;
use notification::Initialized;
use request::RegisterCapability;
use serde::de::Error;
use serde_json::Value;

use crate::{types::types::ProccessId, LanguageSeverProcess};

pub struct LanguageClient {
    server: Arc<LanguageSeverProcess>,
    client_capabilities: ClientCapabilities,
    working_dir: PathBuf,
}

impl LanguageClient {
    pub fn new(
        server: Arc<LanguageSeverProcess>,
        client_capabilities: ClientCapabilities,
        working_dir: &PathBuf,
    ) -> Self {
        Self {
            server: server.clone(),
            client_capabilities,
            working_dir: todo!(),
        }
    }

    async fn new_internal(
        server: Arc<LanguageSeverProcess>,
        client_capabilities: ClientCapabilities,
        process_id: Option<u32>,
        options: Option<Value>,
        working_dir: PathBuf,
        client_info: Option<ClientInfo>,
    ) -> anyhow::Result<()> {
        let root_url = Uri::from_str(working_dir.to_str().unwrap_or_default())
            .unwrap_or_else(|_| Uri::from_str("/").unwrap());
        // Initialize the client
        let init_param = InitializeParams {
            process_id,
            root_path: None,
            root_uri: Some(root_url.clone()),
            initialization_options: options,
            capabilities: client_capabilities,
            trace: None,
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: root_url,
                name: Default::default(),
            }]),
            client_info,
            locale: None,
            ..Default::default()
        };
        if let Err(error) = server.initialize(init_param).await {
            log!(
                log::Level::Error,
                "Error initialize server for client: {:?}",
                error
            );
            return Err(error);
        }

        // Notify Inited
        let initialized_params = InitializedParams {};
        server.notify::<Initialized>(initialized_params).await?;

        let register_params = RegistrationParams {
            registrations: vec![Registration {
                id: todo!(),
                method: todo!(),
                register_options: todo!(),
            }],
        };
        server
            .request::<RegisterCapability>(register_params)
            .await?;
        // Register client capabilities
        Ok(())
    }
}
