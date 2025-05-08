use mpv_client;
use serde_json::json;
use anyhow::{Result, anyhow};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use tokio::sync::broadcast;


macro_rules! unwrap_or_continue {
    ($e:expr) => {
        match $e {
            Ok(val) => val,
            Err(e) => { crate::logger::error!("{e}"); continue },
            
        }
    };
}
pub(crate) use unwrap_or_continue;

#[derive(Debug, Clone)]
pub enum Event {
    Shutdown,
    FileLoaded,
    EndFile,
    Seek, // A seek is started
    PlaybackRestart, // A seek is stopped
    PropertyChange(Property),
}

impl Event {
    pub fn from_mpv_client(value: &mpv_client::Event) -> Result<Option<Self>> {
        let result = match value {
            mpv_client::Event::Shutdown => Event::Shutdown,
            mpv_client::Event::FileLoaded => Event::FileLoaded,
            mpv_client::Event::EndFile(_) => Event::EndFile,
            mpv_client::Event::Seek => Event::Seek,
            mpv_client::Event::PlaybackRestart => Event::Seek,
            mpv_client::Event::PropertyChange(_id, property) => Event::PropertyChange(Property::from_mpv_client_observed(property)?),
            _ => return Ok(None),
        };
        Ok(Some(result))
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

    pub fn status(&mut self) -> serde_json::Value {
        let duration = self.get_property::<f64>("duration").ok();
        let title = self.get_property::<String>("media-title").ok();
        let fullscreen = self.get_property::<bool>("fullscreen").ok();
        let pause = self.get_property::<bool>("pause").ok();
        let time_pos = self.get_property::<f64>("time-pos").ok();
        let volume = self.get_property::<i64>("ao-volume").ok();
        let core_idle = self.get_property::<bool>("core-idle").ok();
        let playlist: Option<serde_json::Value> = match self.get_property::<String>("playlist").ok() {
            Some(s) => serde_json::from_str(s.as_str()).ok(),
            None => None,
        };
        json!({
            "duration": duration,
            "media-title": title,
            "fullscreen": fullscreen,
            "pause": pause,
            "playlist": playlist,
            "time-pos": time_pos,
            "volume": volume,
            "core-idle": core_idle,
        })
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

#[repr(u64)]
pub enum ObservedPropID {
    Pause = 1,
    Fullscreen,
    Playlist,
    Volume,
    TimePos,
    CoreIdle,
}

impl TryFrom<u64> for ObservedPropID {
    type Error = anyhow::Error;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(ObservedPropID::Pause),
            2 => Ok(ObservedPropID::Fullscreen),
            3 => Ok(ObservedPropID::Playlist),
            4 => Ok(ObservedPropID::Volume),
            5 => Ok(ObservedPropID::TimePos),
            6 => Ok(ObservedPropID::CoreIdle),
            n => Err(anyhow!("invalid ObsevedPropID: expected 1-3, found: {n}")),
        }
    }
}

impl TryFrom<&str> for ObservedPropID {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let result = match value {
            "pause" => Self::Pause,
            "fullscreen" => Self::Fullscreen,
            "playlist" => Self::Playlist,
            "ao-volume" => Self::Volume,
            "time-pos" => Self::TimePos,
            "core-idle" => Self::CoreIdle,
            _ => return Err(anyhow!("bad name")),
        };
        Ok(result)
    }
}

impl ToString for ObservedPropID {
    fn to_string(&self) -> String {
        match self {
            Self::Pause => "pause",
            Self::Fullscreen => "fullscreen",
            Self::Playlist => "playlist",
            Self::Volume => "ao-volume",
            Self::TimePos => "time-pos",
            Self::CoreIdle => "core-idle",
        }.to_string()
    }
}

impl ObservedPropID {
    pub fn observe_all(cmd_handle: &mut CmdHandle) -> Result<()> {
        cmd_handle.observe_property::<bool>(Self::Pause as u64, Self::Pause.to_string()).map_err(|e| { anyhow!("{e}") })?;
        cmd_handle.observe_property::<bool>(Self::Fullscreen as u64, Self::Fullscreen.to_string()).map_err(|e| { anyhow!("{e}") })?;
        cmd_handle.observe_property::<String>(Self::Playlist as u64, Self::Playlist.to_string()).map_err(|e| { anyhow!("{e}") })?;
        cmd_handle.observe_property::<bool>(Self::CoreIdle as u64, Self::CoreIdle.to_string()).map_err(|e| { anyhow!("{e}") })?;
        //cmd_handle.observe_property::<i64>(Self::Volume as u64, Self::Volume.to_string()).map_err(|e| { anyhow!("{e}") })?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum Property {
    Pause(bool),
    Fullscreen(bool),
    Playlist(serde_json::Value),
    Volume(i64),
    TimePos(f64),
    CoreIdle(bool),
}

impl Property {
    pub fn name(&self) -> &str {
        match self {
            Self::Pause(_) => "pause",
            Self::Fullscreen(_) => "fullscreen",
            Self::Playlist(_) => "playlist",
            Self::Volume(_) => "ao-volume",
            Self::TimePos(_) => "time-pos",
            Self::CoreIdle(_) => "core-idle",
        } 
    }

    pub fn from_mpv_client_observed(value: &mpv_client::Property) -> Result<Self> {
        let id: ObservedPropID = value.name().try_into()?;
        let result = match id {
            ObservedPropID::Pause => Self::Pause(value.data().ok_or(anyhow!("no value"))?),
            ObservedPropID::Fullscreen => Self::Fullscreen(value.data().ok_or(anyhow!("no value"))?),
            ObservedPropID::Playlist => {
                let data_str: String = value.data().ok_or(anyhow!("no value"))?;
                let data = serde_json::from_str(data_str.as_str()).map_err(|e| {anyhow!(e)})?;
                Self::Playlist(data)
            },
            ObservedPropID::Volume => Self::Volume(value.data().ok_or(anyhow!("no value in volume"))?),
            ObservedPropID::TimePos => Self::TimePos(value.data().ok_or(anyhow!("no value"))?),
            ObservedPropID::CoreIdle => Self::CoreIdle(value.data().ok_or(anyhow!("no value"))?),
        };
        Ok(result)
    }
}
