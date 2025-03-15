mod server;

use mpv_client::{mpv_handle, Event, Handle};
use std::thread;

#[no_mangle]
extern "C" fn mpv_open_cplugin(handle: *mut mpv_handle) -> std::os::raw::c_int {
    let client = Handle::from_ptr(handle);
  
    println!("Hello world from Rust plugin {}!", client.name());
    
    thread::spawn(|| { match server::bind_and_listen() {
        Ok(_) => (),
        Err(e) => println!("Error: {e}"),
    }});
    loop {
        match client.wait_event(-1.) {
            Event::Shutdown => { return 0; },
            event => { println!("Got event: {}", event); },
        }
    }
}

