use std::{path::PathBuf, sync::Arc};

use lsp_types::{ClientCapabilities, InitializeParams};

use crate::LanguageSeverProcess;

pub struct LanguageClient {
    server: Arc<LanguageSeverProcess>,
    client_capabilities: ClientCapabilities,
    working_dir: PathBuf,
}

impl LanguageClient {
    fn new(server: Arc<LanguageSeverProcess>, client_capabilities: ClientCapabilities) -> Self {
        let server = server.clone();
        let working_dir = server.working_dir().clone();
        let init_params = InitializeParams {
            process_id: todo!(),
            root_path: todo!(),
            root_uri: todo!(),
            initialization_options: todo!(),
            capabilities: todo!(),
            trace: todo!(),
            workspace_folders: todo!(),
            client_info: todo!(),
            locale: todo!(),
            work_done_progress_params: todo!(),
        };
        let init_req = server.initialize(params);
        Self {
            server,
            client_capabilities,
            working_dir,
        }
    }
}
