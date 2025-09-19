mod server;
mod websocket;
mod mpv;
mod plugin;
mod config;
mod logger;

use std::net::{IpAddr, Ipv4Addr};

use local_ip_address::linux::local_ip;
use server::Server;
use std::mem;

use mpv_client::{mpv_handle, Event, Handle};
use qrcode;
use image::{self, ImageBuffer};
use mpv::{EventBroadcaster, SplitHandle};
use tokio::runtime::Runtime;

pub fn generate_qr_code_image(data: &str) -> Result<ImageBuffer<image::Rgba<u8>, Vec<u8>>, anyhow::Error> {
    let code = qrcode::QrCode::with_error_correction_level(data.as_bytes(), qrcode::EcLevel::M)?;
    let image = code.render::<image::Rgba<u8>>()
        .min_dimensions(256, 256)
        .build();
    Ok(image)
}

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
    let img = generate_qr_code_image(&format!("hello!")).unwrap();
    let (width, height) = img.dimensions();
    let data = img.into_vec();
    let p = data.as_ptr();
    let _ = handle.command(["overlay-add", 
        "0", //id
        "0", "0", //x, y
        format!("&{:p}", p).as_str(), "0", // pointer, offset
        "bgra", // format
        format!("{}", width).as_str(), format!("{}" ,height).as_str(), // width, height
        format!("{}", width * 4).as_str() // stride
    ]);

    //mem::forget(p);
    let (mut event_handle, mut cmd_handle) = SplitHandle(handle);

    let event_chan = EventBroadcaster::new(32);
    let subscriber = event_chan.subscriber();
    mpv::ObservedPropID::observe_all(&mut cmd_handle).unwrap();

    let rt = Runtime::new().unwrap();
    // webserver
    rt.spawn(async move {
        let server = match Server::bind((config.host.as_str(), config.port)).await {
            Ok(s) => {
                let ip = local_ip().or::<IpAddr>(Ok(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)))).unwrap();
                logger::info!("server started at {}:{}", ip, config.port);
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
