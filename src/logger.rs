use std::sync::Mutex;

static GLOBAL_LOG: Mutex<Logger> = Mutex::new(Logger);

pub struct Logger;
impl Logger {
    fn log(&self, level: LogLevel, arg: &str) {
        println!("[mpvRemote] {} - {}", level, arg)
    }
}

pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Error => "Error",
            Self::Warn => "Warn",
            Self::Info => "Info",
            Self::Debug => "Debug"
        };
        write!(f, "{s}")
    }
}

pub fn log(level: LogLevel, arg: &str) {
    GLOBAL_LOG.lock().unwrap().log(level, arg)
}


macro_rules! error {
    () => {};
    ($($arg:tt)*) => {{
        crate::logger::log(crate::logger::LogLevel::Error, format!($($arg)*).as_str());
    }};
}
pub(crate) use error;

macro_rules! warning {
    () => {};
    ($($arg:tt)*) => {{
        crate::logger::log(crate::logger::LogLevel::Warn, format!($($arg)*).as_str());
    }};
}
pub(crate) use warning;

macro_rules! info{
    () => {};
    ($($arg:tt)*) => {{
        crate::logger::log(crate::logger::LogLevel::Info, format!($($arg)*).as_str());
    }};
}
pub(crate) use info;

macro_rules! debug{
    () => {};
    ($($arg:tt)*) => {{
        crate::logger::log(crate::logger::LogLevel::Debug, format!($($arg)*).as_str());
    }};
}
pub(crate) use debug;
