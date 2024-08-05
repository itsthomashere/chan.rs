use std::{
    collections::HashMap, ffi::OsString, future::Future, path::PathBuf, pin::Pin, sync::Arc,
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};
use tokio::process::Child;

pub const CONTENT_LEN_HEADER: &str = "Content-Length: ";
pub const JSONPRC_VER: &str = "2.0";
pub const HEADER_DELIMITER: &[u8; 4] = b"\r\n\r\n";
pub type NotificationHandler = Box<dyn Send + FnMut(Option<LspRequestId>, Value)>;
pub type ResponseHandler = Box<dyn Send + FnOnce(Result<String, Error>)>;
pub type IoHandler = Box<dyn Send + FnMut(IoKind, &str)>;

#[derive(Debug, Clone, Copy)]
pub enum IoKind {
    StdOut,
    StdIn,
    StdErr,
}
pub struct LanguageServerBinary {
    pub path: PathBuf,
    pub envs: Option<HashMap<String, String>>,
    pub args: Vec<OsString>,
}

#[derive(Debug, Deserialize)]
pub struct ProccessId(pub usize);
#[derive(Debug, Deserialize, Serialize)]
pub struct Error {
    message: String,
}

#[derive(Clone, PartialEq, Eq, Hash, Deserialize, Serialize, Debug)]
#[serde(untagged)]
pub enum LspRequestId {
    Str(String),
    Int(i32),
}

#[derive(Deserialize, Serialize)]
pub struct LspRequest<'a, T> {
    pub jsonprc: &'a str,
    pub id: LspRequestId,
    pub method: &'a str,
    pub params: T,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AnyResponse<'a> {
    pub jsonprc: &'a str,
    pub id: LspRequestId,
    #[serde(default)]
    pub error: Option<Error>,
    #[serde(borrow)]
    pub result: Option<&'a RawValue>,
}

#[derive(Serialize)]
pub struct LspResponse<T> {
    pub jsonprc: &'static str,
    pub id: LspRequestId,
    #[serde(flatten)]
    pub value: LspResult<T>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum LspResult<T> {
    #[serde(rename = "result")]
    Ok(Option<T>),
    Error(Option<Error>),
}

#[derive(Deserialize, Serialize)]
pub struct LspNotification<'a, T> {
    pub jsonprc: &'a str,
    #[serde(borrow)]
    pub method: &'a str,
    pub params: T,
}

#[derive(Debug, Serialize, Clone)]
pub struct AnyNotification {
    #[serde(default)]
    pub id: Option<LspRequestId>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

pub trait LspRequestFuture<O>: Future<Output = O> {
    fn id(&self) -> i32;
}

pub struct FutureRequest<F> {
    id: i32,
    request: F,
}

impl<F> FutureRequest<F> {
    fn new(id: i32, request: F) -> Self {
        Self { id, request }
    }
}

impl<F: Future> Future for FutureRequest<F> {
    type Output = F::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let inner = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().request) };
        inner.poll(cx)
    }
}

impl<F: Future> LspRequestFuture<F::Output> for FutureRequest<F> {
    fn id(&self) -> i32 {
        self.id
    }
}
