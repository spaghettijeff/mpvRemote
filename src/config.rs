use serde_json;
use serde::Deserialize;
use std::io;
use std::path;
use std::sync::LazyLock;
use dirs;

use crate::logger;

#[derive(Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub qr_show: bool,
    pub qr_hide_after_sec: Option<u16>,
    pub qr_hide_after_connect: bool,
}

pub static CONFIG: LazyLock<Config> = LazyLock::new(|| {
    let path = Config::get_path();
    match path {
        Err(e) => {
            logger::warning!("Error loading config file({e}). Using default configuration");
            Config::default()
        }
        Ok(path) => {
            match Config::load(&path) {
                Err(e) => {
                    logger::warning!("Error loading config file({e}). Using default configuration");
                    Config::default()
                }
                Ok(conf) => conf,
            }
        }
    }
});

impl Config {
    pub fn default() -> Self {
        Config {
            host: "0.0.0.0".into(),
            port: 5585,
            qr_show: true,
            qr_hide_after_sec: None,
            qr_hide_after_connect: true,
        }
    }
    pub fn get_path() -> Result<path::PathBuf, io::Error> {
        let conf_dir = dirs::config_dir().ok_or(io::Error::new(io::ErrorKind::Other, "unable to locate config directory"))?;
        let path = conf_dir.join("mpv/script-opts/mpv-remote.json");
        if !path.exists() {
            Err(std::io::Error::new(io::ErrorKind::NotFound, format!("The file: {} does not exist", path.display())))
        } else {
            Ok(path)
        }
    }
    pub fn load(conf_path: &path::PathBuf) -> Result<Self, io::Error> { 
        serde_json::from_str(&std::fs::read_to_string(conf_path)?).map_err(|e| {io::Error::new(io::ErrorKind::Other, e)})
    }
}
