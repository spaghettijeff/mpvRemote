build:
	tailwindcss -i ./www/static/input.css -o ./www/static/output.css

watch:
	tailwindcss --watch -i ./www/static/input.css -o ./www/static/output.css

run:
	$(MAKE) build && mpv --idle --no-audio-display --script=./ --config-dir=./ --msg-level=all=debug test/a.mp3

serve:
	mpv --idle --script=target/debug/libmpv_remote.so
