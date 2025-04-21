# mpvRemote
A simple playback control UI in the web browser for mpv. Supports playback, seeking, and limited playlist editing.
Once the page is served to the client communication is handled by a web socket. This allows for multiple remote devices to connect to the same client while all having control and displaying consistent information.

## Installation
### Release
download the latest release, and copy it into your mpv configuration scripts directory.

### Source
#### Requirements
- tailwindcss
- cargo

#### Building
Build the css using tailwind
```bash
tailwindcss -i ./www/static/input.css -o ./www/static/output.css
```
Then build the binary plugin with cargo
```bash
cargo build
```
copy the compiled binary to your mpv scripts location
```bash
cp target/debug/libmpv_remote.so $XDG_CONFIG_HOME/mpv/scripts/
```

## Usage
Make sure to start mpv with `mpv --idle` so that mpv does not close immediately if you wish to start playback from a remote device.
Connect using a web browser from another device on your network. The default port is 5585.

## Configuration
An example configuration is provided in `script-opts/mpv-remote.json`. You can copy this into your mpv configurations script-opts directory
```bash
mkdir -p "$XDG_CONFIG_HOME/mpv/script-opts/" 
cp "$XDG_CONFIG_HOME/mpv/scripts/mpv-remote/script-opts/mpv-remote.json" "$XDG_CONFIG_HOME/mpv/script-opts/" 
```
