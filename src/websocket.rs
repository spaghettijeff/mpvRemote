use crate::server::{Request, Response};
use std::io::{self, Cursor, Read, Write, Take};
use byteorder::{ByteOrder, NetworkEndian, ReadBytesExt, WriteBytesExt};
use sha1::{self, Digest};
use base64::Engine;

const WS_ACCEPT_CONSTANT: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

#[allow(dead_code)]
pub struct WebSocketClient<T: Read + Write>(T);

#[allow(dead_code)]
pub struct WebSocketServer<T: Read + Write>(T);

#[allow(dead_code)]
impl<T: Read + Write> WebSocketServer<T> {

    pub fn handshake(request: Request, mut stream: T) -> Result<WebSocketServer<T>, io::Error> {
        let ws_key = request.headers.get("Sec-WebSocket-Key")
            .ok_or(io::Error::new(io::ErrorKind::Other, "Sec-WebSocket-Key header not found in request"))?;
        let mut hasher = sha1::Sha1::new();
        hasher.update(ws_key.to_string() + WS_ACCEPT_CONSTANT);
        let ws_accept = base64::engine::general_purpose::STANDARD.encode(&hasher.finalize()[..]);
        let response = Response::new("HTTP/1.1".into(), 101)
            .header("Upgrade", "websocket")
            .header("Connection", "Upgrade")
            .header("Sec-WebSocket-Accept", &ws_accept);
        stream.write_all(&response.bytes())?;
        Ok(WebSocketServer(stream))
    }

    pub fn get_message<'a>(&'a mut self) -> Result<Message<Frame<'a, T>>, io::Error> {
        let mut frame = Frame::deserialize(&mut self.0)?;
        let mut len = frame.payload_len;
        let m_type = match frame.opcode {
            OpCode::Text => MessageType::Text,
            OpCode::Binary => MessageType::Binary,
            OpCode::Ping => MessageType::Ping,
            OpCode::Pong => MessageType::Pong,
            OpCode::Close => {
                let code = frame.read_u16::<NetworkEndian>()?;
                len -= size_of::<CloseStatus>() as u64;
                MessageType::Close(code)
            },
            OpCode::Cont => todo!(),
        };
        Ok(Message{
            data: frame.take(len),
            r#type: m_type,
        })
    }

    pub fn send_message<R: Read>(&mut self, mut msg: Message<R>) -> Result<u64, io::Error> {
        let opcode: OpCode = msg.r#type.into();
        let payload_len = msg.data.limit();
        let frame = Frame {
            fin: true,
            opcode,
            payload_len,
            masking_key: None,
            payload: &mut msg.data,
        };
        let mut frame_data = frame.serialize();
        io::copy(&mut frame_data, &mut self.0)
    }
}

#[derive(Debug)]
pub struct Message<T: Read> {
    data: Take<T>,
    r#type: MessageType,
}

type CloseStatus = u16;
#[derive(Debug, Clone, Copy)]
enum MessageType {
    Text,
    Binary,
    Ping,
    Pong,
    Close(CloseStatus),
}

impl From<MessageType> for OpCode {
    fn from(value: MessageType) -> Self {
        match value {
            MessageType::Text => OpCode::Text,
            MessageType::Binary => OpCode::Binary,
            MessageType::Ping => OpCode::Ping,
            MessageType::Pong => OpCode::Pong,
            MessageType::Close(_) => OpCode::Close,
        }
    }
}

impl<'a, T: Read> Read for Message<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.data.read(buf)
    }
}

impl<'a, T: Read> Drop for Message<T> {
    fn drop(&mut self) {
        let _ = io::copy(&mut self.data, &mut io::sink());
    }
}

impl<'a> From<&'a str> for Message<&'a[u8]> {
    fn from(value: &'a str) -> Self {
        let len = value.len() as u64;
        let bytes = value.as_bytes();
        Message {
            r#type: MessageType::Text,
            data: bytes.take(len),
        }
    }
}

impl<'a> From<&'a[u8]> for Message<&'a[u8]> {
    fn from(value: &'a[u8]) -> Self {
        let len = value.len() as u64;
        Message {
            r#type: MessageType::Binary,
            data: value.take(len),
        }
    }
}

#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum OpCode {
    Cont = 0x0,
    Text = 0x1,
    Binary = 0x2,
    Close = 0x8,
    Ping = 0x9,
    Pong = 0xA,
}

impl TryFrom<u8> for OpCode {
    type Error = io::Error;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use OpCode::*;
        match value { // disgusting there must be a better way
            x if x == Cont as u8 => Ok(Cont),
            x if x == Text as u8 => Ok(Text),
            x if x == Binary as u8 => Ok(Binary),
            x if x == Ping as u8 => Ok(Ping),
            x if x == Pong as u8 => Ok(Pong),
            x => Err(io::Error::new(io::ErrorKind::Other, format!("illegal Op Code: {x}"))),
        }
    }
}

#[derive(Debug)]
pub struct Frame<'a, T> {
    fin: bool,
    opcode: OpCode,
    payload_len: u64,
    masking_key: Option<u32>,
    payload: &'a mut T,
}

impl<'a, T: Read> Read for Frame<'a, T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.masking_key {
            Some(mask) => {
            let mut mask_bytes: [u8; 4] = [0; 4];
            NetworkEndian::write_u32(&mut mask_bytes, mask);
            let read_result = (*self.payload).read(buf)?;
            for i in 0..read_result {
                buf[i] = buf[i] ^ mask_bytes[i % 4];
            }
            Ok(read_result)
            },
            None => (*self.payload).read(buf),
        }
    }
}

impl<'a, T: Read> Frame<'a, T> {
    fn deserialize(stream: &'a mut T) -> Result<Frame<'a, T>, io::Error> {
        let mut buffer = [0; 8];
        stream.take(2).read(&mut buffer)?;
        let fin: bool = (0b10000000 & buffer[0]) != 0;
        let opcode: OpCode = (0b01111111 & buffer[0]).try_into()?;

        let mask: bool = (0b10000000 & buffer[1]) != 0;
        let payload_len: u64 = match 0b01111111 & buffer[1] {
            126 => {
                stream.take(2).read(&mut buffer)?;
                NetworkEndian::read_u16(&buffer) as u64
            },
            127 => {
                stream.take(8).read(&mut buffer)?;
                NetworkEndian::read_u64(&buffer) as u64
            },
            len => len as u64,
        };

        let masking_key = if mask {
            stream.take(4).read(&mut buffer)?;
            Some(NetworkEndian::read_u32(&buffer) as u32)
        } else { 
            None
        };
        Ok(Frame{
            fin,
            opcode,
            payload_len,
            masking_key,
            payload: stream,
        })
    }

    fn serialize(self) -> std::io::Chain<Cursor<Vec<u8>>, Self> {
        let mask = matches!(self.masking_key, Some(_));
        let mut header_bytes: Vec<u8> = Vec::new();
        let byte_1 = if self.fin { 0b10000000 } else { 0 } | self.opcode as u8;
        header_bytes.push(byte_1);

        if self.payload_len <= 125 { // 7 bit payload length
            let byte = if mask { 0b10000000 } else { 0 } | self.payload_len as u8;
            header_bytes.push(byte);
        } else if self.payload_len <= u16::MAX as u64 { // 16 bit payload length
            let byte = if mask { 0b10000000 } else { 0 } | 126 as u8;
            header_bytes.push(byte);
            header_bytes.write_u16::<NetworkEndian>(self.payload_len as u16).unwrap();

        } else { // 64 bit payload length
            let byte = if mask { 0b10000000 } else { 0 } | 127 as u8;
            header_bytes.push(byte);
            header_bytes.write_u64::<NetworkEndian>(self.payload_len as u64).unwrap();
        }
        match self.masking_key {
            Some(key) => header_bytes.write_u32::<NetworkEndian>(key).unwrap(),
            None => (),
        }
        Cursor::new(header_bytes).chain(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_deserialize() {
        let frame_bin = vec![
            0b10000010, // fin = 1, opcode = 2 (binary)
            0b00000100, // mask = 0, payload_len = 4
            1, 2, 3, 4, // payload
        ];
        let mut data = std::io::Cursor::new(frame_bin);
        let frame = Frame::deserialize(&mut data).unwrap();
        assert!(frame.fin == true);
        assert!(frame.opcode == OpCode::Binary);
        assert!(frame.masking_key == None);
        assert!(frame.payload_len == 4);
    }

    #[test]
    fn frame_serialize() {
        let s = "hello world";
        let mut payload = Cursor::new(s);
        let frame = Frame{
            fin: true,
            opcode: OpCode::Text,
            payload_len: s.len() as u64,
            masking_key: None,
            payload: &mut payload,
        };
        let mut buf = Vec::new();
        frame.serialize().read_to_end(&mut buf).unwrap();
        println!("{:#?}", buf);
        let mut r = Cursor::new(buf);
        let f = Frame::deserialize(&mut r).unwrap();
        println!("{:#?}", f);
    }

    #[test]
    fn frame_tcp_stream_no_mask() {
        const ADDR: &str = "127.0.0.1:9898";
        const PAYLOAD_STR: &str = "test payload";
        let listener = std::net::TcpListener::bind(ADDR).unwrap();
        let th = std::thread::spawn(move || {
            let mut payload = Cursor::new(PAYLOAD_STR);
            let send_frame = Frame{
                fin: true,
                opcode: OpCode::Text,
                payload_len: PAYLOAD_STR.len() as u64,
                masking_key: None,
                payload: &mut payload,
            };
            let mut f = send_frame.serialize();
            let mut sender = std::net::TcpStream::connect(ADDR).unwrap();
            io::copy(&mut f, &mut sender).unwrap();
        });
        let mut stream = listener.incoming().next().unwrap().unwrap();
        let mut out_frame = Frame::deserialize(&mut stream).unwrap();
        println!("{:#?}", out_frame);
        let mut buf = String::new();
        let n = out_frame.read_to_string(&mut buf).unwrap();
        println!("Read {n} bytes:\t{buf}");
        assert_eq!(buf, PAYLOAD_STR);

        let _ = th.join();
    }

    #[test]
    fn frame_tcp_stream_mask() {
        const ADDR: &str = "127.0.0.1:9899";
        const PAYLOAD_STR: &str = "test payload";
        let listener = std::net::TcpListener::bind(ADDR).unwrap();
        let th = std::thread::spawn(move || {
            let mut payload = Cursor::new(PAYLOAD_STR);
            let send_frame = Frame{
                fin: true,
                opcode: OpCode::Text,
                payload_len: PAYLOAD_STR.len() as u64,
                masking_key: Some(0xa3ff0792 as u32), // 100% genuine random mask
                payload: &mut payload,
            };
            let mut f = send_frame.serialize();
            let mut sender = std::net::TcpStream::connect(ADDR).unwrap();
            io::copy(&mut f, &mut sender).unwrap();
        });
        let mut stream = listener.incoming().next().unwrap().unwrap();
        let mut out_frame = Frame::deserialize(&mut stream).unwrap();
        println!("{:#?}", out_frame);
        let mut buf = String::new();
        let n = out_frame.read_to_string(&mut buf).unwrap();
        println!("Read {n} bytes:\t{buf}");
        assert_eq!(buf, PAYLOAD_STR);

        let _ = th.join();
    }
}
