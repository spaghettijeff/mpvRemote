use crate::server::{Request, Response};
use std::io::{self, Cursor, Read, Write};
use byteorder::{ByteOrder, NetworkEndian, WriteBytesExt};

pub enum WebSocket<T: Read + Write> {
    Server(T),
    Client(T),
}

impl<T: Read + Write> WebSocket<T> {
    fn read_message(&self) -> Message {
        todo!()
    }
    fn write_message(&self, msg: Message) -> () {
        todo!()
    }
}

pub enum Message {
    Text,
    Bytes,
}

#[repr(u8)]
#[derive(Debug, PartialEq, Clone, Copy)]
enum OpCode {
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
struct Frame<'a, T> {
    fin: bool,
    opcode: OpCode,
    mask: bool, // TODO remove this in favor of key option
    payload_len: usize,
    masking_key: Option<u32>,
    payload: &'a mut T,
}

impl<'a, T: Read> Read for Frame<'a, T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.mask {
            let read_result = (*self.payload).read(buf)?;
            let mask = unsafe {
                std::mem::transmute::<u32, [u8; 4]>
                    (self.masking_key.unwrap()) // panic if mask is true but key is none
            };
            for i in 0..read_result {
                buf[i] = buf[i] ^ mask[i % 4];
            }
            Ok(read_result)
        } else {
            (*self.payload).read(buf)
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
        let payload_len: usize = match 0b01111111 & buffer[1] {
            126 => {
                stream.take(2).read(&mut buffer)?;
                NetworkEndian::read_u16(&buffer) as usize
            },
            127 => {
                stream.take(8).read(&mut buffer)?;
                NetworkEndian::read_u64(&buffer) as usize
            },
            len => len as usize,
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
            mask,
            payload_len,
            masking_key,
            payload: stream,
        })
    }

    fn serialize(&'a mut self) -> std::io::Chain<Cursor<Vec<u8>>, &'a mut Self> {
        let mut header_bytes: Vec<u8> = Vec::new();
        let byte_1 = if self.fin { 0b10000000 } else { 0 } | self.opcode as u8;
        header_bytes.push(byte_1);

        if self.payload_len <= 125 { // 7 bit payload length
            let byte = if self.mask { 0b10000000 } else { 0 } | self.payload_len as u8;
            header_bytes.push(byte);
        } else if self.payload_len <= u16::MAX as usize { // 16 bit payload length
            let byte = if self.mask { 0b10000000 } else { 0 } | 126 as u8;
            header_bytes.push(byte);
            header_bytes.write_u16::<NetworkEndian>(self.payload_len as u16).unwrap();

        } else { // 64 bit payload length
            let byte = if self.mask { 0b10000000 } else { 0 } | 127 as u8;
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
        assert!(frame.mask == false);
        assert!(frame.masking_key == None);
        assert!(frame.payload_len == 4);
    }

    #[test]
    fn frame_serialize() {
        let s = "hello world";
        let mut payload = Cursor::new(s);
        let mut frame = Frame{
            fin: true,
            opcode: OpCode::Text,
            mask: false,
            payload_len: s.len(),
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
            let mut send_frame = Frame{
                fin: true,
                opcode: OpCode::Text,
                mask: false,
                payload_len: PAYLOAD_STR.len(),
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
            let mut send_frame = Frame{
                fin: true,
                opcode: OpCode::Text,
                mask: true,
                payload_len: PAYLOAD_STR.len(),
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
