# mpvRemote
A simple playback control UI in the web browser for mpv. Supports playback, seeking, and limited playlist editing.
Once the page is served to the client communication is handled by a web socket. This allows for multiple remote devices to connect to the same client while all having control and displaying consistent information.

## Requirements
- [mpv](https://mpv.io/)
- [lua-http](https://github.com/daurnimator/lua-http)
- ip and awk (optional, used to display the address on the local network)

## Installation
Clone the repository into your mpv configuration scripts directory
```bash
git clone https://github.com/spaghettijeff/mpvRemote "$XDG_CONFIG_HOME/mpv/scripts/mpv-remote"
```

## Usage
On startup a qr code will be displayed that can be used to connect to the client via the local network (this can be disabled in the settings).
Make sure to start mpv with `mpv --idle` so that mpv does not close immediately if you wish to start playback from a remote device.

## Configuration
An example configuration is provided in `script-opts/mpv-remote.conf`. You can copy this into your mpv configurations script-opts directory
```bash
mkdir -p "$XDG_CONFIG_HOME/mpv/script-opts/" 
cp "$XDG_CONFIG_HOME/mpv/scripts/mpv-remote/script-opts/mpv-remote.conf" "$XDG_CONFIG_HOME/mpv/script-opts/" 
```
