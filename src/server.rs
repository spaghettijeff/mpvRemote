use core::str;
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};


#[derive(Debug)]
pub struct Request {
    method: Method,
    path: String,
    ver: String,
    headers: HashMap<String, String>,
    body: Option<String>,
}

impl Request {
    fn parse(mut lines: BufReader<&TcpStream>) -> Result<Request, io::Error> {
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
            Some(content_len) => {
                let content_len = content_len.parse::<usize>().map_err(|e| { io::Error::new(io::ErrorKind::Other, e.to_string()) })?;
                let mut buf = vec![0u8; content_len];
                lines.read_exact(&mut buf)?;
                let s = str::from_utf8(&buf).map_err(|e| { io::Error::new(io::ErrorKind::Other, e.to_string()) })?;
                Some(s.to_string())
            },
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


pub fn bind_and_listen() -> Result<(), io::Error>{
    let listener = TcpListener::bind("0.0.0.0:8080")?;
    for stream in listener.incoming() {
        match stream {
            Err(e) => {
                println!("Error:\t{e:#?}");
            },
            Ok(mut stream) => {
                let reader = io::BufReader::new(&stream);
                let request = Request::parse(reader);
                println!("{request:#?}");
                let r = Response::new("HTTP/1.1".into(), 200)
                    .header("Content-Type".into(), "text/text".into())
                    .body("penis".into());
                let r = r.to_string();
                println!("{r}");
                stream.write_all(r.as_bytes())?
            },
        }
    }
    return Ok(())
}

struct Response{ //TODO change the Strings to &str probably?
    version: String,
    status: u16,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

impl ToString for Response {
    fn to_string(&self) -> String {
        let mut resp = format!("{} {}\n", self.version, self.status).to_string();
        self.headers.iter().for_each(|item| { 
            resp += format!("{}: {}\n", item.0, item.1).as_str();
        });
        resp += "\r\n";
        if let Some(body) = &self.body {
            resp += body;
        }
        resp
    }
}

impl Response {
    fn new(version: String, status: u16) -> Response {
        Response { version , status, headers: Vec::new(), body: None }
    }

    fn header(mut self, key: String, value: String) -> Response {
        self.headers.push((key, value));
        self
    }

    fn body(mut self, body: String) -> Response {
        self.body = Some(body);
        self
    }
}


