use core::str;
use std::io;
use std::collections::HashMap;
use std::fmt::Debug;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use crate::websocket;


const INDEX_HTML: &[u8] = include_bytes!("../www/index.html");
const MAIN_JS: &[u8] = include_bytes!("../www/static/main.js");
const OUTPUT_CSS: &[u8] = include_bytes!("../www/static/output.css");
const SYMBOLS_FONT: &[u8] = include_bytes!("../www/static/symbols/material-symbols.woff2");

macro_rules! continue_on_err {
    ($expression:expr) => {
        match $expression {
            Ok(val) => val,
            Err(e) => { println!("Error:\t{e:#?}"); continue; },
        }
    };
}

#[derive(Debug)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub ver: String,
    pub headers: HashMap<String, String>,
}

impl Request {
    async fn parse<T>(stream: &mut T) -> Result<Request, io::Error> 
    where 
    T: AsyncRead + Unpin,
    {
        let mut lines = BufReader::new(stream);
        let mut buf = String::new();
        let first_line  = match lines.read_line(&mut buf).await {
            Ok(0) => { return Err(io::Error::new(io::ErrorKind::Other, "unexpected EOF in http request")) },
            Err(e) => { return Err(e) },
            Ok(_len) => &buf,
        };
        let (method, path, ver) = match first_line.split_whitespace().collect::<Vec<&str>>()[..] {
            [method, path, ver] => (method.to_string(), path.to_string(), ver.to_string()),
            _ => return Err(io::Error::new(io::ErrorKind::Other, format!("invalid http request: \"{first_line}\""))),
        };

        let mut headers: HashMap<String, String> = HashMap::new();
        loop {
            buf.clear();
            let num_read = lines.read_line(&mut buf).await;
            if buf == "\r\n" { break }
            let (key, val) = match num_read {
                Ok(0) => { return Err(io::Error::new(io::ErrorKind::Other, "unexpected EOF in http request")) },
                Err(e) => { return Err(e) },
                Ok(_len) => buf.split_once(':').ok_or(io::Error::new(io::ErrorKind::Other, format!("invalid header in http request \"{buf}\"")))?,
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

impl TryFrom<&str> for Method {
    type Error = io::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
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
            method => Err(io::Error::new(io::ErrorKind::Other, format!("invalid method {method}"))),
        }
    }
}

#[derive(Debug)]
pub struct Response<'a> {
    version: String,
    status: u16,
    headers: Vec<(String, String)>,
    body: Option<&'a [u8]>,
}

impl<'a> Response<'a> {
    pub fn new(version: &str, status: u16) -> Response<'a> {
        Response {
            version: version.into(), 
            status, 
            headers: Vec::new(),
            body: None
        }
    }

    pub fn header(mut self, key: &str, value: &str) -> Response<'a> {
        self.headers.push((key.into(), value.into()));
        self
    }

    pub fn body(mut self, body: &'a[u8]) -> Response {
        self.body = Some(body);
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
            [header, body].concat()
        } else {
            header.to_owned()
        }
    }
}

pub async fn bind_and_listen() -> Result<(), io::Error>{
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    loop {
        let (mut stream, _addr) = continue_on_err!(listener.accept().await);
        tokio::spawn(async {
            let request = Request::parse(&mut stream).await.unwrap();
            dbg!(&request);
            let _ = handle_request(request, stream).await;
        });
    }
}

async fn handle_request<T>(request: Request, mut stream: T) -> Result<(), io::Error>
where
    T: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    match request.path.as_str() {
        "/" => {
            let response = Response::new("HTTP/1.1".into(), 200)
                .header("Content-Type".into(), "text/html".into())
                .body(INDEX_HTML);
            stream.write_all(&response.bytes()).await
            },
        "/static/main.js" => {
            let response = Response::new("HTTP/1.1".into(), 200)
                .header("Content-Type".into(), "text/javascript".into())
                .body(MAIN_JS);
            stream.write_all(&response.bytes()).await
            },
        "/static/output.css" => {
            let response = Response::new("HTTP/1.1".into(), 200)
                .header("Content-Type".into(), "text/css".into())
                .body(OUTPUT_CSS);
            stream.write_all(&response.bytes()).await
            },
        "/static/symbols/material-symbols.woff2" => {
            let response = Response::new("HTTP/1.1".into(), 200)
                .header("Content-Type".into(), "font/woff2".into())
                .body(SYMBOLS_FONT);
            stream.write_all(&response.bytes()).await
            },
        "/socket" => {
            tokio::spawn(async move {
                let mut ws = websocket::WebSocketServer::handshake(request, stream).await?;
                loop {
                    let mut msg = ws.get_message().await?;
                    let mut data = String::new();
                    msg.read_to_string(&mut data).await?;
                    ws.send_message(data.as_str().into()).await?;
                }
                Ok::<(), io::Error>(())
            });
            Ok(())
        },
        _path => {
            let response = Response::new("HTTP/1.1".into(), 404)
                .header("Content-Type".into(), "text/html".into());
            stream.write_all(&response.bytes()).await
            },
    }
}
