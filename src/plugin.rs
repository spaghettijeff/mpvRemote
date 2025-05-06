use std::path::Path;
use std::env::current_dir;
use tokio::sync::broadcast;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use serde_json::{json, Value};
use serde::{Serialize, Deserialize};
use anyhow::{Result, anyhow, bail};

use crate::mpv::{self, ObservedPropID, Event, Property, CmdHandle};
use crate::websocket::WebSocketServer;

#[derive(Debug, Serialize, Deserialize)]
struct WebEvent {
    event: String,
    data: Option<Value>,
}

pub async fn handle_client_connection<T>(
    mut ws: WebSocketServer<T>, 
    cmd_handle: &mut CmdHandle<'_>, 
    mut event_chan: broadcast::Receiver<Event>) -> Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin
{
    let mut msg_buffer = String::new();
    loop {
        msg_buffer.clear();
        tokio::select! {
            mpv_msg = event_chan.recv() => {
                let mpv_msg = mpv_msg?;
                match mpv_msg {
                    Event::PropertyChange(property) => {
                        match property {
                            mpv::Property::Pause(val) => {
                                let payload = json!({
                                    "event": property.name(),
                                    "data": val,
                                });
                                let payload_str = serde_json::to_string(&payload)?;
                                ws.send_message(payload_str.as_str().into()).await?;
                            },
                            Property::Fullscreen(val) => {
                                let payload = json!({
                                    "event": property.name(),
                                    "data": val,
                                });
                                let payload_str = serde_json::to_string(&payload)?;
                                ws.send_message(payload_str.as_str().into()).await?;
                            },
                            Property::Playlist(ref val) => {
                                let payload = json!({
                                    "event": property.name(),
                                    "data": val,
                                });
                                let payload_str = serde_json::to_string(&payload)?;
                                ws.send_message(payload_str.as_str().into()).await?;
                            },
                            Property::TimePos(val) => {
                                let payload = json!({
                                    "event": property.name(),
                                    "data": val,
                                });
                                let payload_str = serde_json::to_string(&payload)?;
                                ws.send_message(payload_str.as_str().into()).await?;
                            },
                            _ => (),
                        }
                    },
                    Event::FileLoaded => {
                        let stat = cmd_handle.status();
                        let response = json!({
                            "event": "status",
                            "data": stat
                        });
                        let msg = mpv::unwrap_or_continue!(serde_json::to_string(&response));
                        ws.send_message(msg.as_str().into()).await?;
                    },
                    Event::Seek => {
                        let time_pos = match cmd_handle.get_property::<f64>("time-pos") {
                            Ok(time) => time,
                            Err(_) => continue,
                        };
                        let payload = json!({
                            "event": "time-pos",
                            "data": time_pos,
                        });
                        let payload_str = serde_json::to_string(&payload)?;
                        ws.send_message(payload_str.as_str().into()).await?;
                    },
                    _ => ()
                };
            },
            client_msg = ws.get_message() => {
                let mut client_msg = client_msg?;
                let _ = mpv::unwrap_or_continue!(client_msg.read_to_string(&mut msg_buffer).await);
                let msg: WebEvent = mpv::unwrap_or_continue!(serde_json::from_str(msg_buffer.as_str()));
                handle_webclient(msg, cmd_handle, &mut ws).await?;
            },
        }
    }
}

async fn handle_webclient<T>(payload: WebEvent, handle: &mut CmdHandle<'_>, ws: &mut WebSocketServer<T>) -> Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin
{
    match payload.event.as_str() {
        "toggle-play" => {
            let paused: bool = handle.get_property("pause").map_err(|e| { anyhow!(e) })?;
            handle.set_property("pause", !paused);
        },
        "toggle-fullscreen" => {
            let fullscreen: bool = handle.get_property("fullscreen").map_err(|e| { anyhow!(e) })?;
            handle.set_property("fullscreen", !fullscreen);
        },
        "volume" => {
            let vol = match payload.data {
                Some(Value::String(n)) => n,
                _ => bail!("volume data not found in message \"{payload:?}\""),
            }.parse::<i64>()?;
            // volume cannot be observed until audio is loaded
            // TODO fix this 
            let _ = handle.observe_property::<i64>(
                ObservedPropID::Volume as u64,
                ObservedPropID::Volume.to_string());
            handle.set_property("ao-volume", vol);
        },
        "get-status" => {
            let stat =  handle.status();
            let response = json!({
                "event": "status",
                "data": stat
            });
            ws.send_message(serde_json::to_string(&response).unwrap().as_str().into()).await?;
            },
        "seek" => {
            let data = match payload.data {
                Some(Value::Object(ref v)) => v,
                _ => bail!("seek data not found in message \"{payload:?}\""),
            };
            if let Some(Value::Number(n)) = data.get("relative") {
                handle.command(["seek", format!("{}", n).as_str(), "relative"]);
            } else if let Some(Value::Number(n)) = data.get("absolute") {
                handle.command(["seek", format!("{}", n).as_str(), "absolute"]);
            } else {
                bail!("seek data not found in message \"{payload:?}\"");
            }
        },
        "skip" => {
            let data = match payload.data {
                Some(Value::String(n)) => n,
                _ => bail!("skip data not found in message \"{payload:?}\""),
            };
            handle.command([format!("playlist-{data}")]);
        },
        "play-now" => {
            let data = match payload.data {
                Some(Value::Object(v)) => v,
                _ => bail!("play-now data not found in message \"{payload:?}\""),
            };
            if let Some(Value::String(url)) = data.get("url") {
                handle.command(["loadfile", url, "replace"]);
            } 
            if let Some(Value::Object(file)) = data.get("file") {
                let dir = file.get("dir").ok_or(anyhow!("directory not found"))?;
                let name = file.get("name").ok_or(anyhow!("file name not found"))?;
                let mut path = current_dir()?;
                path.push(Path::new(dir.as_str().ok_or(anyhow!(""))?));
                path.push(Path::new(name.as_str().ok_or(anyhow!(""))?));
                handle.command(["loadfile", path.to_str().ok_or(anyhow!(""))?, "replace"]);
            }
        },
        "playlist-add" => {
            let data = match payload.data {
                Some(Value::Object(v)) => v,
                _ => bail!("playist-add data not found in message \"{payload:?}\""),
            };
            if let Some(Value::String(url)) = data.get("url") {
                handle.command(["loadfile", url, "append-play"]);
            } 
            if let Some(Value::Object(file)) = data.get("file") {
                let dir = file.get("dir").ok_or(anyhow!("directory not found"))?;
                let name = file.get("name").ok_or(anyhow!("file name not found"))?;
                let mut path = current_dir()?;
                path.push(Path::new(dir.as_str().ok_or(anyhow!(""))?));
                path.push(Path::new(name.as_str().ok_or(anyhow!(""))?));
                handle.command(["loadfile", path.to_str().ok_or(anyhow!(""))?, "append-play"]);
            }
        },
        "playlist-remove" => {
            let idx = match payload.data {
                Some(Value::Number(n)) => n.as_i64().unwrap(),
                _ => bail!("playist-remove data not found in message \"{payload:?}\""),
            };
            handle.command(["playlist-remove", &format!("{idx}")]);
        },
        "playlist-move" => {
            let ids = match payload.data {
                Some(Value::Array(arr)) => arr,
                _ => bail!("playist-move data not found in message \"{payload:?}\""),
            };
            let id_1 = ids[0].as_i64().ok_or(anyhow!("unable to parse integer"))?;
            let mut id_2 = ids[1].as_i64().ok_or(anyhow!("unable to parse integer"))?;
            if id_1 < id_2 {
                id_2 += 1;
            }
            handle.command(["playlist-move", &format!("{id_1}"), &format!("{id_2}")]);
        },
        "shutdown" => {
            handle.command(["quit"]);
        },
        "stop" => {
            handle.command(["write-watch-later-config"]);
            handle.command(["stop"]);
        },
        _ => (),
    }
    Ok(())
}
