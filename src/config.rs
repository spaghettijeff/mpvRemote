use serde_json;
use serde::Deserialize;
use std::io;
use dirs;

#[derive(Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u16,
}

impl Config {
    pub fn default() -> Self {
        Config {
            host: "0.0.0.0".into(),
            port: 5585,
        }
    }
    pub fn load() -> Result<Self, io::Error> { 
        let conf_dir = dirs::config_dir().ok_or(io::Error::new(io::ErrorKind::Other, "unable to locate config directory"))?;
        let conf = conf_dir.join("mpv/script-opts/mpv-remote.json");
        serde_json::from_str(&std::fs::read_to_string(conf)?).map_err(|e| {io::Error::new(io::ErrorKind::Other, e)})
    }
}
