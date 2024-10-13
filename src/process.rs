use std::{collections::HashMap, ffi::OsString, path::PathBuf};

use lsp_types::{notification, request};

pub struct LanguageServerBinary {
    pub path: PathBuf,
    pub envs: Option<HashMap<String, String>>,
    pub args: Vec<OsString>,
}

pub struct LanguageServer {}

impl LanguageServer {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {})
    }

    pub async fn request<T: request::Request>(
        &self,
        params: T::Params,
    ) -> anyhow::Result<T::Result> {
        todo!()
    }

    pub async fn notify<T: notification::Notification>(
        &self,
        params: T::Params,
    ) -> anyhow::Result<()> {
        todo!()
    }

    pub fn on_request(&self) {
        todo!()
    }

    pub fn on_notification(&self) {
        todo!()
    }

    pub fn on_io(&self) {
        todo!()
    }

    pub fn server_id(&self) {
        todo!()
    }

    pub fn root_path(&self) {
        todo!()
    }

    pub fn working_dir(&self) {
        todo!()
    }

    pub fn capabilities(&self) {
        todo!()
    }

    pub fn update_capabilities(&mut self) {}

    pub fn code_action_kinds(&self) {
        todo!()
    }

    pub fn name(&self) {
        todo!()
    }
}
