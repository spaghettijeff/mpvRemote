use mpv_client;
use std::io;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::io::{AsyncWrite, AsyncRead};

use crate::websocket::WebSocketServer;


#[repr(u64)]
enum ObservedPropID {
    Pause = 1,
    Fullscreen,
    Playlist,
}

impl TryFrom<u64> for ObservedPropID {
    type Error = io::Error;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(ObservedPropID::Pause),
            2 => Ok(ObservedPropID::Fullscreen),
            3 => Ok(ObservedPropID::Playlist),
            n => Err(io::Error::new(io::ErrorKind::Other, format!("invalid ObsevedPropID: expected 1-3, found: {n}"))),
        }
    }
}

impl ToString for ObservedPropID {
    fn to_string(&self) -> String {
        match self {
            Self::Pause => "pause",
            Self::Fullscreen => "fullscreen",
            Self::Playlist => "playlist",
        }.to_string()
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Shutdown,
    StartFile(i64), // playlist index
    EndFile,
    Seek, // A seek is started
    PlaybackRestart, // A seek is stopped
    PropertyChange(PropertyEvent),
}

#[derive(Debug, Clone)]
pub enum PropertyData {
    String(String),
    Bool(bool),
    Int(i64),
    Float(f64),
}

#[derive(Debug, Clone)]
pub struct PropertyEvent {
    name: String,
    data: PropertyData,
}

impl PropertyEvent {
    fn from_mpv_client(value: &mpv_client::Property, id: u64) -> Result<Self, io::Error> {
        let id = ObservedPropID::try_from(id)?;
        let result = match id {
            ObservedPropID::Pause => PropertyEvent { 
                name: id.to_string(), 
                data: PropertyData::Bool(value.data::<bool>().ok_or(io::Error::new(io::ErrorKind::Other, "expected observed property \"pause\" to have type bool"))?),
            },
            ObservedPropID::Fullscreen => PropertyEvent { 
                name: id.to_string(), 
                data: PropertyData::Bool(value.data::<bool>().ok_or(io::Error::new(io::ErrorKind::Other, "expected observed property \"fullscreen\" to have type bool"))?),
            },
            ObservedPropID::Playlist => PropertyEvent { 
                name: id.to_string(), 
                data: PropertyData::String(value.data::<String>().ok_or(io::Error::new(io::ErrorKind::Other, "expected observed property \"playlist\" to have type String"))?),
            },
        };
        Ok(result)
    }
}

impl Event {
    pub fn from_mpv_client(value: &mpv_client::Event) -> Option<Self> {
        let result = match value {
            mpv_client::Event::Shutdown => Event::Shutdown,
            mpv_client::Event::StartFile(start) => Event::StartFile(start.playlist_entry_id()),
            mpv_client::Event::EndFile(_) => Event::EndFile,
            mpv_client::Event::Seek => Event::Seek,
            mpv_client::Event::PlaybackRestart => Event::Seek,
            mpv_client::Event::PropertyChange(id, property) => Event::PropertyChange(PropertyEvent::from_mpv_client(&property, *id).unwrap()),
            _ => return None
        };
        Some(result)
    }
}

#[derive(Clone)]
pub struct EventBroadcaster(Arc<broadcast::Sender<Event>>);
pub type EventSubscriber = Arc<dyn Fn() -> broadcast::Receiver<Event> + Send + Sync>;

impl EventBroadcaster {
    pub fn new(buf_size: usize) -> Self {
        EventBroadcaster(Arc::new(broadcast::Sender::new(buf_size)))
    }

    pub fn subscriber(&self) -> EventSubscriber {
        let subscriber_generator = self.0.clone();
        Arc::new(move || { subscriber_generator.as_ref().subscribe() })
    }
}

impl core::ops::Deref for EventBroadcaster {
    type Target = broadcast::Sender<Event>;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

pub struct CmdHandle<'a>(&'a mut mpv_client::Handle);

impl<'a> CmdHandle<'a> {
    /// DO NOT CALL WILL PANIC
    /// this is a hack on a hack
    /// CmdHandle wraps mpv_client::Handle and should be able to call all methods except this one as it
    /// is not thread safe. The function calls cannot be wrapped manually due to the mpv_client::Format
    /// trait not being exported. Deref is used to get around, but that would also expose 
    /// mpv_client::Handle::wait_event method. This is here to overwrite that function. The body of
    /// this function contains a single panic!()
    #[deprecated]
    #[allow(dead_code)]
    pub fn wait_event(&mut self, _timeout: f64) -> mpv_client::Event {
        panic!()
    }
}

impl<'a> Deref for CmdHandle<'a> {
    type Target = mpv_client::Handle;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

// This is fine according to the mpv docs (mpv/include/mpv/client.h - line 133)
unsafe impl<'a> Send for CmdHandle<'a> {}
unsafe impl<'a> Sync for CmdHandle<'a> {}

pub struct EventHandle<'a>(&'a mut mpv_client::Handle);

impl<'a> EventHandle<'a> {
    pub fn wait_event(&mut self, timeout: f64) -> mpv_client::Event {
        self.0.wait_event(timeout)
    }
}

pub fn SplitHandle(handle: &mut mpv_client::Handle) -> (EventHandle, CmdHandle) {
    unsafe {
        let ptr = handle.as_mut_ptr();
        (EventHandle(mpv_client::Handle::from_ptr(ptr.clone())),
         CmdHandle(mpv_client::Handle::from_ptr(ptr)))
    }
    
}

pub async fn handle_client_connection<T>(
    mut ws: WebSocketServer<T>, 
    cmd_handle: Arc<CmdHandle<'_>>, 
    mut event_chan: broadcast::Receiver<Event>) -> Result<(), io::Error>
where
    T: AsyncRead + AsyncWrite + Unpin
{
    loop {
        tokio::select! {
            mpv_msg = event_chan.recv() => {
                println!("MPV Event");
            },
            client_msg = ws.get_message() => {
                println!("Client Event")
            },
        }
    }
    Ok(())
}
