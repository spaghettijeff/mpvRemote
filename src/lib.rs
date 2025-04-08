mod server;
mod websocket;
mod plugin;

use mpv_client::{mpv_handle, Event, Handle};
use plugin::EventBroadcaster;
use tokio::runtime::Runtime;
use std::sync::{Arc, Mutex};

#[no_mangle]
extern "C" fn mpv_open_cplugin(handle: *mut mpv_handle) -> std::os::raw::c_int {
    let client = Handle::from_ptr(handle);
    println!("Hello world from Rust plugin {}!", client.name());
    let rt = Runtime::new().unwrap();

    let event_chan = EventBroadcaster::new(32);
    let subscriber = event_chan.subscriber();
    rt.spawn(async move {
        match server::bind_and_listen(subscriber).await {
        Ok(_) => (),
        Err(e) => println!("Error: {e}"),
        }
    });

    loop {
        match client.wait_event(-1.) {
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
