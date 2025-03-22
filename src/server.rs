use core::str;
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};


const index_html: &[u8] = include_bytes!("../www/index.html");
const main_js: &[u8] = include_bytes!("../www/static/main.js");
const output_css: &[u8] = include_bytes!("../www/static/output.css");
const symbols_font: &[u8] = include_bytes!("../www/static/symbols/material-symbols.woff2");

macro_rules! continue_on_err {
    ($expression:expr) => {
        match $expression {
            Ok(val) => val,
            Err(e) => { println!("Error:\t{e:#?}"); continue; },
        }
    };
}

#[derive(Debug)]
pub struct Request<'a, T: std::io::Read> {
    method: Method,
    path: String,
    ver: String,
    headers: HashMap<String, String>,
    body: Option<&'a T>,
}

impl<'a, T: std::io::Read> Request<'a, T> where
    &'a T: std::io::Read {
    fn parse(stream: &'a T) -> Result<Request<T>, io::Error> {
        let mut lines = BufReader::new(stream);
        let mut buf = String::new();
        let first_line  = match lines.read_line(&mut buf) {
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
            let num_read = lines.read_line(&mut buf);
            print!("{buf}");
            if buf == "\r\n" { break }
            let (key, val) = match num_read {
                Ok(0) => { return Err(io::Error::new(io::ErrorKind::Other, "unexpected EOF in http request")) },
                Err(e) => { return Err(e) },
                Ok(_len) => buf.split_once(':').ok_or(io::Error::new(io::ErrorKind::Other, format!("invalid header in http request \"{buf}\"")))?,
            };
        headers.insert(key.to_string(), val.trim().to_string());
        }
        buf.clear();
        let body = match headers.get("Content-Length") {
            Some(_) => Some(stream),
            None => None,
        };
        Ok(Request { 
            method: Method::try_from(method.as_str())?,
            path: path.to_owned(), 
            ver: ver.to_owned(),
            headers,
            body,
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

pub struct Response<'a> {
    version: String,
    status: u16,
    headers: Vec<(String, String)>,
    body: Option<&'a [u8]>,
}

impl<'a> Response<'a> {
    fn new(version: String, status: u16) -> Response<'a> {
        Response { version , status, headers: Vec::new(), body: None }
    }

    fn header(mut self, key: String, value: String) -> Response<'a> {
        self.headers.push((key, value));
        self
    }

    fn body(mut self, body: &'a[u8]) -> Response {
        self.body = Some(body);
        self
    }

    fn bytes(self) -> Vec<u8> {
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

pub fn bind_and_listen() -> Result<(), io::Error>{
    let listener = TcpListener::bind("0.0.0.0:8080")?;
    for stream in listener.incoming() {
        let mut stream = continue_on_err!(stream);
        let request = continue_on_err!(Request::parse(&stream));
        println!("{request:#?}");

        let response = handle_request(request);
        continue_on_err!(stream.write_all(&response.bytes()));
    }
    Ok(())
}

fn handle_request<T: std::io::Read>(request: Request<T>) -> Response {
    match request.path.as_str() {
        "/" => Response::new("HTTP/1.1".into(), 200)
            .header("Content-Type".into(), "text/html".into())
            .body(index_html),
        "/static/main.js" => Response::new("HTTP/1.1".into(), 200)
            .header("Content-Type".into(), "text/javascript".into())
            .body(main_js),
        "/static/output.css" => Response::new("HTTP/1.1".into(), 200)
            .header("Content-Type".into(), "text/css".into())
            .body(output_css),
        "/static/symbols/material-symbols.woff2" => Response::new("HTTP/1.1".into(), 200)
            .header("Content-Type".into(), "font/woff2".into())
            .body(symbols_font),
        _path => Response::new("HTTP/1.1".into(), 404)
            .header("Content-Type".into(), "text/html".into()),
    }
}

