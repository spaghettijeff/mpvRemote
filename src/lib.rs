mod server;
mod websocket;
mod mpv;
mod plugin;
mod config;
mod logger;
mod qr_overlay;

use std::net::{IpAddr, Ipv4Addr};

use local_ip_address::linux::local_ip;
use qr_overlay::QR_OVERLAY;
use server::Server;

use mpv_client::{mpv_handle, Event, Handle};
use mpv::{EventBroadcaster, SplitHandle};
use tokio::runtime::Runtime;

#[unsafe(no_mangle)]
extern "C" fn mpv_open_cplugin(handle: *mut mpv_handle) -> std::os::raw::c_int {
    logger::set_log_level(logger::LogLevel::Debug);
    let config = match config::Config::get_path() {
        Ok(path) => {
            match config::Config::load(&path) {
                Ok(conf) => conf,
                Err(e) => {
                    logger::warning!("Failed to load config file. Using default options ( in {}: {e})", path.display());
                    config::Config::default()
                },
            }
        },
        Err(e) => {
            logger::warning!("Config file not found. Using default options ({e})");
            config::Config::default()
        }
    };
    let handle = Handle::from_ptr(handle);

    let (mut event_handle, mut cmd_handle) = SplitHandle(handle);

    let event_chan = EventBroadcaster::new(32);
    let subscriber = event_chan.subscriber();
    mpv::ObservedPropID::observe_all(&mut cmd_handle).unwrap();

    let rt = Runtime::new().unwrap();
    // webserver
    let _http_server_handle = rt.spawn(async move {
        let server = match Server::bind((config.host.as_str(), config.port)).await {
            Ok(s) => {
                let ip = local_ip().unwrap_or(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)));
                logger::info!("server started at {}:{}", ip, config.port);
                QR_OVERLAY.lock().unwrap().initialize(&format!("{}:{}", ip, config.port), cmd_handle.clone());
                QR_OVERLAY.lock().unwrap().toggle_overlay();
                s
            },
            Err(e) => {
                logger::error!("failed to start server: {e}");
                return
            },
        };
        let _ = server.listen(cmd_handle, subscriber).await;
    });
    // mpv event loop
    loop {
        match event_handle.wait_event(-1.) {
            Event::Shutdown => {
                return 0;
            },
            evt => {
                let evt = mpv::unwrap_or_continue!(mpv::Event::from_mpv_client(&evt));
                match evt {
                    Some(e) => {
                        let _ = event_chan.send(e);
                    },
                    None => continue,
                }
            },
        }
    }
}
