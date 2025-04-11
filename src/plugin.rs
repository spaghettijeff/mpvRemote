use mpv_client;
use std::{i64, io};
use std::path::Path;
use std::env::current_dir;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use serde_json::{json, Value};
use serde::{Serialize, Deserialize};


use crate::websocket::WebSocketServer;

#[repr(u64)]
pub enum ObservedPropID {
    Pause = 1,
    Fullscreen,
    Playlist,
    Volume,
}

impl TryFrom<u64> for ObservedPropID {
    type Error = io::Error;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(ObservedPropID::Pause),
            2 => Ok(ObservedPropID::Fullscreen),
            3 => Ok(ObservedPropID::Playlist),
            4 => Ok(ObservedPropID::Volume),
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
            Self::Volume => "ao-volume",
        }.to_string()
    }
}

impl ObservedPropID {
    pub fn observe_all(cmd_handle: &mut CmdHandle) -> mpv_client::Result<()> {
        cmd_handle.observe_property::<bool>(Self::Pause as u64, Self::Pause.to_string())?;
        cmd_handle.observe_property::<bool>(Self::Fullscreen as u64, Self::Fullscreen.to_string())?;
        cmd_handle.observe_property::<String>(Self::Playlist as u64, Self::Playlist.to_string())?;
        //cmd_handle.observe_property::<i64>(Self::Volume as u64, Self::Volume.to_string())?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Shutdown,
    FileLoaded,
    EndFile,
    Seek, // A seek is started
    PlaybackRestart, // A seek is stopped
    PropertyChange(PropertyEvent),
    PlaybackTime(f64),
}

#[derive(Debug, Clone)]
pub enum PropertyData {
    String(String),
    Bool(bool),
    Int(i64),
    Float(f64),
}

impl std::fmt::Display for PropertyData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(w) => write!(f, "{w}"),
            Self::Bool(w) => write!(f, "{w}"),
            Self::Int(w) => write!(f, "{w}"),
            Self::Float(w) => write!(f, "{w}"),
        }
    }
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
            ObservedPropID::Volume => PropertyEvent { 
                name: id.to_string(), 
                data: PropertyData::Int(value.data::<i64>()
                    .ok_or(io::Error::new(
                            io::ErrorKind::Other, 
                            "expected observed property \"volume\" to have type i64"))?),
            },
        };
        Ok(result)
    }
}

impl Event {
    pub fn from_mpv_client(value: &mpv_client::Event) -> Option<Self> {
        let result = match value {
            mpv_client::Event::Shutdown => Event::Shutdown,
            mpv_client::Event::FileLoaded => Event::FileLoaded,
            mpv_client::Event::EndFile(_) => Event::EndFile,
            mpv_client::Event::Seek => Event::Seek,
            mpv_client::Event::PlaybackRestart => Event::Seek,
            mpv_client::Event::PropertyChange(id, property) => {
                let e = PropertyEvent::from_mpv_client(&property, *id);
                match e {
                    Ok(e) => Event::PropertyChange(e),
                    Err(_) => return None,
                }
            }
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

impl<'a> DerefMut for CmdHandle<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl<'a> Clone for CmdHandle<'a> {
    fn clone(&self) -> Self {
        unsafe {
            let ptr = self.0.as_ptr().clone();
            CmdHandle(mpv_client::Handle::from_ptr(ptr.cast_mut()))
        }
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

#[derive(Debug, Serialize, Deserialize)]
struct WebEvent {
    event: String,
    data: Option<Value>,
}

pub async fn handle_client_connection<T>(
    mut ws: WebSocketServer<T>, 
    cmd_handle: &mut CmdHandle<'_>, 
    mut event_chan: broadcast::Receiver<Event>) -> Result<(), io::Error>
where
    T: AsyncRead + AsyncWrite + Unpin
{
    let mut msg_buffer = String::new();
    loop {
        msg_buffer.clear();
        tokio::select! {
            mpv_msg = event_chan.recv() => {
                let mpv_msg = mpv_msg.unwrap();
                match mpv_msg {
                    Event::PropertyChange(PropertyEvent { name, data }) => {
                        match name.as_str() {
                            "pause" => {
                                let data = match data {
                                    PropertyData::Bool(b) => b,
                                    _ => continue,
                                };
                                let payload = json!({
                                    "event": name,
                                    "data": data,
                                });
                                let payload_str = serde_json::to_string(&payload).unwrap();
                                ws.send_message(payload_str.as_str().into()).await;
                            },
                            "fullscreen" => {
                                let data = match data {
                                    PropertyData::Bool(b) => b,
                                    _ => continue,
                                };
                                let payload = json!({
                                    "event": name,
                                    "data": data,
                                });
                                let payload_str = serde_json::to_string(&payload).unwrap();
                                ws.send_message(payload_str.as_str().into()).await;
                            },
                            "file-loaded" => {
                            },
                            "playlist" => {
                                let data = match data {
                                    PropertyData::String(s) => s,
                                    _ => continue,
                                };
                                let playlist_json: Value = serde_json::from_str(&data).unwrap();
                                let payload = json!({
                                    "event": name,
                                    "data": playlist_json,
                                });
                                let payload_str = serde_json::to_string(&payload).unwrap();
                                ws.send_message(payload_str.as_str().into()).await;
                            },
                            _ => (),
                        }
                    },
                    Event::FileLoaded => {
                        let duration = cmd_handle.get_property::<f64>("duration").unwrap();
                        let title = cmd_handle.get_property::<String>("media-title").unwrap();
                        let fullscreen = cmd_handle.get_property::<bool>("fullscreen").unwrap();
                        let pause = cmd_handle.get_property::<bool>("pause").unwrap();
                        let playlist: Value = serde_json::from_str(&cmd_handle.get_property::<String>("playlist").unwrap()).unwrap();
                        let time_pos = cmd_handle.get_property::<f64>("time-pos").unwrap();
                        let volume = cmd_handle.get_property::<i64>("ao-volume").unwrap();
                        let response = json!({
                            "event": "status",
                            "data": {
                                "duration": duration,
                                "media-title": title,
                                "fullscreen": fullscreen,
                                "pause": pause,
                                "playlist": playlist,
                                "time-pos": time_pos,
                                "volume": volume,
                            }
                        });
                        ws.send_message(serde_json::to_string(&response).unwrap().as_str().into()).await;
                    },
                    Event::PlaybackTime(time) => {
                        let payload = json!({
                            "event": "time-pos",
                            "data": time,
                        });
                        let payload_str = serde_json::to_string(&payload).unwrap();
                        ws.send_message(payload_str.as_str().into()).await;

                    }
                    _ => ()
                };
            },
            client_msg = ws.get_message() => {
                match client_msg.unwrap().read_to_string(&mut msg_buffer).await {
                    Ok(_) => (),
                    Err(_) => continue,
                };
                let msg: WebEvent = serde_json::from_str(msg_buffer.as_str()).unwrap();
                handle_webclient(msg, cmd_handle, &mut ws).await;
            },
        }
    }
}

async fn handle_webclient<T>(payload: WebEvent, handle: &mut CmdHandle<'_>, ws: &mut WebSocketServer<T>) 
where
    T: AsyncRead + AsyncWrite + Unpin
{
    match payload.event.as_str() {
        "toggle-play" => {
            let paused: bool = handle.get_property("pause").unwrap();
            handle.set_property("pause", !paused);
        },
        "toggle-fullscreen" => {
            let fullscreen: bool = handle.get_property("fullscreen").unwrap();
            handle.set_property("fullscreen", !fullscreen);
        },
        "volume" => {
            let vol = match payload.data {
                Some(Value::String(n)) => n,
                _ => return,
            }.parse::<i64>().unwrap();
            // volume cannot be observed until audio is loaded
            // TODO fix this 
            let _ = handle.observe_property::<i64>(
                ObservedPropID::Volume as u64,
                ObservedPropID::Volume.to_string());
            handle.set_property("ao-volume", vol);
        },
        "get-status" => {
            let duration = handle.get_property::<f64>("duration").unwrap();
            let title = handle.get_property::<String>("media-title").unwrap();
            let fullscreen = handle.get_property::<bool>("fullscreen").unwrap();
            let pause = handle.get_property::<bool>("pause").unwrap();
            let playlist: Value = serde_json::from_str(&handle.get_property::<String>("playlist").unwrap()).unwrap();
            let time_pos = handle.get_property::<f64>("time-pos").unwrap();
            let volume = handle.get_property::<i64>("ao-volume").unwrap();
            let response = json!({
                "event": "status",
                "data": {
                    "duration": duration,
                    "media-title": title,
                    "fullscreen": fullscreen,
                    "pause": pause,
                    "playlist": playlist,
                    "time-pos": time_pos,
                    "volume": volume,
                }
            });
            ws.send_message(serde_json::to_string(&response).unwrap().as_str().into()).await;
            },
        "seek" => {
            let data = match payload.data {
                Some(Value::Object(v)) => v,
                _ => return,
            };
            if let Some(Value::Number(n)) = data.get("relative") {
                handle.command(["seek", format!("{}", n).as_str(), "relative"]);
            } else if let Some(Value::Number(n)) = data.get("absolute") {
                handle.command(["seek", format!("{}", n).as_str(), "absolute"]);
            } else {
                return
            }
        },
        "skip" => {
            let data = match payload.data {
                Some(Value::String(n)) => n,
                _ => return,
            };
            handle.command([format!("playlist-{data}")]);
        },
        "play-now" => {
            let data = match payload.data {
                Some(Value::Object(v)) => v,
                _ => return,
            };
            if let Some(Value::String(url)) = data.get("url") {
                handle.command(["loadfile", url, "replace"]);
            } 
            if let Some(Value::Object(file)) = data.get("file") {
                let dir = file.get("dir").unwrap();
                let name = file.get("name").unwrap();
                let mut path = current_dir().unwrap();
                path.push(Path::new(dir.as_str().unwrap()));
                path.push(Path::new(name.as_str().unwrap()));
                handle.command(["loadfile", path.to_str().unwrap(), "replace"]);
            }
        },
        "playlist-add" => {
            let data = match payload.data {
                Some(Value::Object(v)) => v,
                _ => return,
            };
            if let Some(Value::String(url)) = data.get("url") {
                handle.command(["loadfile", url, "append-play"]);
            } 
            if let Some(Value::Object(file)) = data.get("file") {
                let dir = file.get("dir").unwrap();
                let name = file.get("name").unwrap();
                let mut path = current_dir().unwrap();
                path.push(Path::new(dir.as_str().unwrap()));
                path.push(Path::new(name.as_str().unwrap()));
                handle.command(["loadfile", path.to_str().unwrap(), "append-play"]);
            }
        },
        "playlist-remove" => {
            let idx = match payload.data {
                Some(Value::Number(n)) => n.as_i64().unwrap(),
                _ => return,
            };
            handle.command(["playlist-remove", &format!("{idx}")]);
        },
        "playlist-move" => {
            let ids = match payload.data {
                Some(Value::Array(arr)) => arr,
                _ => return,
            };
            let id_1 = ids[0].as_i64().unwrap();
            let mut id_2 = ids[1].as_i64().unwrap();
            if id_1 < id_2 {
                id_2 += 1;
            }
            handle.command(["playlist-move", &format!("{id_1}"), &format!("{id_2}")]);
        },
        "shutdown" => {
            handle.command(["quit"]);
        },
        "stop" => {
            handle.command(["write-watch-later-config"]);
            handle.command(["stop"]);
        },
        _ => return,
    }
}
