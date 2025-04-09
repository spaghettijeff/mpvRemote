mod server;
mod websocket;
mod plugin;

use mpv_client::{mpv_handle, Event, Handle};
use plugin::{EventBroadcaster, SplitHandle};
use tokio::runtime::Runtime;

#[no_mangle]
extern "C" fn mpv_open_cplugin(handle: *mut mpv_handle) -> std::os::raw::c_int {
    let handle = Handle::from_ptr(handle);
    let (mut event_handle, cmd_handle) = SplitHandle(handle);
    let event_chan = EventBroadcaster::new(32);
    let subscriber = event_chan.subscriber();

    let rt = Runtime::new().unwrap();
    rt.spawn(async move {
        match server::bind_and_listen(cmd_handle, subscriber).await {
        Ok(_) => (),
        Err(e) => println!("Error: {e}"),
        }
    });

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
