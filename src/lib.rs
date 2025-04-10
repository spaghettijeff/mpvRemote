mod server;
mod websocket;
mod plugin;

use mpv_client::{mpv_handle, Event, Handle};
use plugin::{EventBroadcaster, SplitHandle};
use tokio::runtime::Runtime;
use tokio::time::{self, Duration};

#[no_mangle]
extern "C" fn mpv_open_cplugin(handle: *mut mpv_handle) -> std::os::raw::c_int {
    let handle = Handle::from_ptr(handle);
    let (mut event_handle, mut cmd_handle) = SplitHandle(handle);

    let event_chan = EventBroadcaster::new(32);
    let subscriber = event_chan.subscriber();
    plugin::ObservedPropID::observe_all(&mut cmd_handle).unwrap();

    let rt = Runtime::new().unwrap();
    // playback time
    let timer_broadcaster = event_chan.clone();
    let mut timer_handle = cmd_handle.clone();
    rt.spawn(async move {
        let mut interval = time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let time_pos = match timer_handle.get_property::<f64>("time-pos") {
                Ok(time) => time,
                Err(_) => continue,
            };
            timer_broadcaster.send(plugin::Event::PlaybackTime(time_pos));
        }
    });
    // webserver
    rt.spawn(async move {
        match server::bind_and_listen(cmd_handle, subscriber).await {
        Ok(_) => (),
        Err(e) => println!("Error: {e}"),
        }
    });
    // mpv event loop
    loop {
        match event_handle.wait_event(-1.) {
            Event::Shutdown => return 0,
            evt => {
                let evt = plugin::Event::from_mpv_client(&evt);
                match evt {
                    Some(e) => {
                        let _ = dbg!(event_chan.send(e));
                    },
                    None => continue,
                }
            },
        }
    }
}
