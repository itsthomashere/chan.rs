use std::{
    collections::HashMap, ffi::OsString, future::Future, path::PathBuf, pin::Pin, time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};

const JSON_RPC_VERSION: &str = "2.0";
const CONTENT_LEN_HEADER: &str = "";
const LSP_REQ_TIMEOUT: Duration = Duration::from_secs(120);
const SERVER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy)]
enum IoKind {
    StdIn,
    StdOut,
    StdErr,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LsBin {
    path: PathBuf,
    args: Vec<OsString>,
    env: Option<HashMap<String, String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct LsId(pub usize);

#[derive(Debug, Deserialize, Serialize)]
pub struct LspError {
    message: String,
}

// This section is implementation of Language server Protocol RPC
//
// To read more about specification: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification)

// For the request id
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RequestId {
    Int(i32),
    Str(String),
}

// LSP Request
//
// Request message json format
#[derive(Deserialize, Serialize)]
pub struct Request<'a, T> {
    jsonrpc: &'a str,
    id: RequestId,
    method: &'a str,
    params: T,
}

// The response before deserialize into proper format
#[derive(Deserialize, Serialize)]
pub struct AnyResponse<'a> {
    jsonrpc: &'a str,
    id: RequestId,
    #[serde(default)]
    error: Option<LspError>,
    #[serde(borrow)]
    results: Option<&'a RawValue>,
}

// Helper for parsing result with either error or result value
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum LspResult<T> {
    #[serde(rename = "result")]
    Ok(Option<T>),
    Error(Option<LspError>),
}

// LSP Response concrete format
//
// Response in json format
#[derive(Serialize)]
pub struct Response<T> {
    jsonrpc: &'static str,
    id: RequestId,
    #[serde(flatten)]
    value: LspResult<T>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnyNotification {
    #[serde(default)]
    id: Option<RequestId>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

// LSP Notification concrete format
//
// Notification in json
#[derive(Deserialize, Serialize)]
pub struct Notification<'a, T> {
    jsonrpc: &'a str,
    #[serde(borrow)]
    method: &'a str,
    params: T,
}

pub trait LspRequestFuture<O>: Future<Output = O> {
    fn id(&self) -> i32;
}

struct LspRequest<F> {
    id: i32,
    request: F,
}
impl<F> LspRequest<F> {
    pub fn new(id: i32, request: F) -> Self {
        Self { id, request }
    }
}

impl<F: Future> Future for LspRequest<F> {
    type Output = F::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let inner = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().request) };
        inner.poll(cx)
    }
}
impl<F: Future> LspRequestFuture<F::Output> for LspRequest<F> {
    fn id(&self) -> i32 {
        self.id
    }
}
