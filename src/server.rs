use core::str;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::path;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::fs;
use anyhow::{Result, anyhow};
use crate::logger::{debug, warning, error};
use crate::qr_overlay::QR_OVERLAY;
use crate::{config, logger, plugin, websocket};
use crate::mpv::{CmdHandle, EventSubscriber};


const INDEX_HTML: &[u8] = include_bytes!("../www/index.html");
const MAIN_JS: &[u8] = include_bytes!("../www/static/main.js");
const OUTPUT_CSS: &[u8] = include_bytes!("../www/static/output.css");
const SYMBOLS_FONT: &[u8] = include_bytes!("../www/static/symbols/material-symbols.woff2");

macro_rules! continue_on_err {
    ($expression:expr) => {
        match $expression {
            Ok(val) => val,
            Err(e) => {
                logger::error!("{e}");
                continue;
            },
        }
    };
}

pub type Url<'a> = Vec<&'a str>;

pub fn parse_url<'a>(url: &'a str) -> Url<'a> {
    url.split("/").collect()
}

#[derive(Debug)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub ver: String,
    pub headers: HashMap<String, String>,
}

impl Request {
    pub fn new(path: String, method: Method) -> Request {
        Request { 
            method, 
            path, 
            ver: "HTTP/1.1".to_owned(), 
            headers: HashMap::new(),
        }
    }

    pub fn header(mut self, key: &str, val: &str) -> Request {
        self.headers.insert(key.into(), val.into());
        self
    }

    pub fn bytes(self) -> Vec<u8> {
        let mut header = format!("{} {} {}\n", self.method, self.path ,self.ver).to_string();
        self.headers.iter().for_each(|item| { 
            header += format!("{}: {}\n", item.0, item.1).as_str();
        });
        header += "\r\n";
        header.as_bytes().to_vec()
    }

    pub async fn parse<T>(stream: &mut T) -> Result<Request> 
    where 
    T: AsyncRead + Unpin,
    {
        let mut lines = BufReader::new(stream);
        let mut buf = String::new();
        let first_line  = match lines.read_line(&mut buf).await {
            Ok(0) => { return Err(anyhow!("unexpected EOF in http request")) },
            Err(e) => { return Err(e.into()) },
            Ok(_len) => &buf,
        };
        let (method, path, ver) = match first_line.split_whitespace().collect::<Vec<&str>>()[..] {
            [method, path, ver] => (method.to_string(), path.to_string(), ver.to_string()),
            _ => return Err(anyhow!("invalid http request")),
        };

        let mut headers: HashMap<String, String> = HashMap::new();
        loop {
            buf.clear();
            let num_read = lines.read_line(&mut buf).await;
            if buf == "\r\n" { break }
            let (key, val) = match num_read {
                Ok(0) => { return Err(anyhow!("unexpected EOF in http request")) },
                Err(e) => { return Err(e.into()) },
                Ok(_len) => buf.split_once(':').ok_or(anyhow!("invalid header in http request"))?,
            };
        headers.insert(key.to_string(), val.trim().to_string());
        }
        buf.clear();
        Ok(Request { 
            method: Method::try_from(method.as_str())?,
            path: path.to_owned(), 
            ver: ver.to_owned(),
            headers,
        })
    }
}

#[derive(Debug)]
pub enum Method {
    GET,
    HEAD,
    POST,
    PUT,
    DELETE,
    CONNECT,
    OPTIONS,
    TRACE,
    PATCH,
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Self::GET => "GET",
            Self::HEAD => "HEAD",
            Self::POST => "POST",
            Self::PUT => "PUT",
            Self::DELETE => "DELETE",
            Self::CONNECT => "CONNECT",
            Self::OPTIONS => "OPTIONS",
            Self::TRACE => "TRACE",
            Self::PATCH => "PATCH",
        };
        f.write_str(str)
    }
}

impl TryFrom<&str> for Method {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        use Method::*;
        match value {
            "GET" => Ok(GET),
            "HEAD" => Ok(HEAD),
            "POST" => Ok(POST),
            "PUT" => Ok(PUT),
            "DELETE" => Ok(DELETE),
            "CONNECT" => Ok(CONNECT),
            "OPTIONS" => Ok(OPTIONS),
            "TRACE" => Ok(TRACE),
            "PATCH" => Ok(PATCH),
            method => Err(anyhow!("invalid HTTP method: {method}")),
        }
    }
}

#[derive(Debug)]
pub struct Response {
    pub version: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

impl<'a> Response {
    pub fn new(version: &str, status: u16) -> Response {
        Response {
            version: version.into(), 
            status, 
            headers: Vec::new(),
            body: None
        }
    }

    pub fn header(mut self, key: &str, value: &str) -> Response {
        self.headers.push((key.into(), value.into()));
        self
    }

    pub fn body(mut self, body: &[u8]) -> Response {
        self.body = Some(body.to_owned());
        self
    }

    pub fn bytes(self) -> Vec<u8> {
        let mut header = format!("{} {}\n", self.version, self.status).to_string();
        self.headers.iter().for_each(|item| { 
            header += format!("{}: {}\n", item.0, item.1).as_str();
        });
        header += "\r\n";
        let header = header.as_bytes();
        if let Some(body) = self.body {
            [header, &body].concat()
        } else {
            header.to_owned()
        }
    }

    pub async fn parse<T>(stream: &mut T) -> Result<Response> 
    where 
    T: AsyncRead + Unpin,
    {
        let mut lines = BufReader::new(stream);
        let mut buf = String::new();
        let first_line  = match lines.read_line(&mut buf).await {
            Ok(0) => { return Err(anyhow!("unexpected EOF in http request")) },
            Err(e) => { return Err(e.into()) },
            Ok(_len) => &buf,
        };
        let (version, status) = match first_line.split_whitespace().collect::<Vec<&str>>()[..] {
            [version, status] => (version.to_string(), status.parse::<u16>()?),
            _ => return Err(anyhow!("invalid http request")),
        };

        let mut headers: Vec<(String, String)> = Vec::new();
        loop {
            buf.clear();
            let num_read = lines.read_line(&mut buf).await;
            if buf == "\r\n" { break }
            let (key, val) = match num_read {
                Ok(0) => { return Err(anyhow!("unexpected EOF in http request")) },
                Err(e) => { return Err(e.into()) },
                Ok(_len) => buf.split_once(':').ok_or(anyhow!("invalid header in http request"))?,
            };
        headers.push((key.to_string(), val.trim().to_string()));
        }
        buf.clear();
        let _ = lines.read_line(&mut buf).await?; //lfcr
        buf.clear();
        let body_len = lines.read_to_string(&mut buf).await?;
        let body = if body_len == 0 { None } else { Some(buf.as_bytes().to_owned()) };
        Ok(Response {
            version,
            status,
            headers,
            body
        })
    }
}

pub struct Server(TcpListener);

impl Server {
    pub async fn bind<A>(addr: A) -> Result<Self>
    where 
        A: tokio::net::ToSocketAddrs
    {
        Ok(Self(TcpListener::bind(addr).await.map_err(|e| {anyhow!(e)})?))
    }

    pub async fn listen(&self, cmd_handle: CmdHandle<'static>, subscriber: EventSubscriber) -> Result<()> {
        loop {
            let cmd_handle = cmd_handle.clone();
            let (mut stream, _addr) = continue_on_err!(self.0.accept().await);
            let sub = subscriber.clone();
            tokio::spawn(async move {
                let request = Request::parse(&mut stream).await.unwrap();
                let _ = Self::handle_request(request, stream, cmd_handle, sub).await;
            });
        }
    }

    async fn handle_request<T>(request: Request, mut stream: T, mut cmd_handle: CmdHandle<'static>, subscriber: EventSubscriber) -> Result<()>
    where
        T: AsyncRead + AsyncWrite + Send + Unpin + 'static + Debug,
    {
        let url = parse_url(&request.path);
        match &url[1 ..] {
            [""] | ["", ""] => {
                let response = Response::new("HTTP/1.1".into(), 200)
                    .header("Content-Type".into(), "text/html".into())
                    .body(INDEX_HTML);
                stream.write_all(&response.bytes()).await?;
                Ok(())
            },
            ["static", "main.js"] => {
                let response = Response::new("HTTP/1.1".into(), 200)
                    .header("Content-Type".into(), "text/javascript".into())
                    .body(MAIN_JS);
                stream.write_all(&response.bytes()).await?;
                Ok(())
            },
            ["static", "output.css"] => {
                let response = Response::new("HTTP/1.1".into(), 200)
                    .header("Content-Type".into(), "text/css".into())
                    .body(OUTPUT_CSS);
                stream.write_all(&response.bytes()).await?;
                Ok(())
            },
            ["static", "symbols", "material-symbols.woff2"] => {
                let response = Response::new("HTTP/1.1".into(), 200)
                    .header("Content-Type".into(), "font/woff2".into())
                    .body(SYMBOLS_FONT);
                stream.write_all(&response.bytes()).await?;
                Ok(())
            },
            ["socket"] => {
                tokio::spawn(async move {
                    let ws = websocket::WebSocketServer::handshake(request, stream).await?;
                    logger::debug!("new websocket connection: {ws:?}");
                    let _ = QR_OVERLAY.lock().unwrap().hide_overlay();
                    plugin::handle_client_connection(ws, &mut cmd_handle, subscriber()).await?;
                    Ok::<(), anyhow::Error>(())
                });
                Ok(())
            },
            ["file-picker", rest @ ..] => {
                let mut fpath = std::env::current_dir()?;
                for f in rest {
                    fpath.push(f);
                }
                let mut entries = fs::read_dir(fpath).await?;
                let mut dirs :Vec<String> = Vec::new();
                let mut files :Vec<String> = Vec::new();
                while let Some(entry) = entries.next_entry().await? {
                    let name = entry.file_name().into_string().map_err(|e| {anyhow!("unable to format OsString \"{e:?}\"")})?;
                    if entry.path().is_dir() {
                        dirs.push(name);
                    } else if entry.path().is_file() {
                        files.push(name);
                    } else {
                        panic!("{entry:?} is not a file or dir");
                    }
                }
                let payload = json!({
                    "dirs": dirs,
                    "files": files
                }).to_string();
                debug!("file picker {payload}");
                let response = Response::new("HTTP/1.1".into(), 200)
                    .header("Content-Type".into(), "application/json".into())
                    .body(payload.as_bytes());
                stream.write_all(&response.bytes()).await?;
                Ok(())
            },
            path => {
                warning!("bad request path not found \"{path:?}\"");
                let response = Response::new("HTTP/1.1".into(), 404)
                    .header("Content-Type".into(), "text/html".into());
                stream.write_all(&response.bytes()).await?;
                Ok(())
            },
        }
    }
}

