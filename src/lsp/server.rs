use async_trait::async_trait;
use lsp_types::{
    request::{GotoDeclarationParams, GotoDeclarationResponse},
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse, InitializeParams,
    InitializedParams, RenameParams, TextDocumentPositionParams,
};

#[async_trait]
trait LanguageServer {
    async fn initialize(&self, params: InitializeParams);
    async fn initialized(&self, params: InitializedParams);
    async fn shutdown(&self);
    async fn exit(&self);
    async fn did_open(&self, params: DidOpenTextDocumentParams);
    async fn did_close(&self, params: DidCloseTextDocumentParams);
    async fn did_change(&self, params: DidChangeTextDocumentParams);
    async fn did_save(&self, params: DidSaveTextDocumentParams);
    async fn rename(&self, params: RenameParams);
    async fn prepare_rename(&self, params: TextDocumentPositionParams);
    async fn definition(
        &self,
        params: GotoDefinitionParams,
    ) -> anyhow::Result<GotoDefinitionResponse>;
    async fn declaration(
        &self,
        params: GotoDeclarationParams,
    ) -> anyhow::Result<GotoDeclarationResponse>;
}
