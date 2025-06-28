build: css
	cargo build

css:
	tailwindcss -i ./www/static/input.css -o ./www/static/output.css

serve:
	mpv --idle --script=target/debug/libmpv_remote.so

