local cqueues = require "cqueues"
local msg = require "mp.msg"

local options = {
    host = "0.0.0.0";
    port = 5585;
    show_qr = true;
}
require "mp.options".read_options(options, "mpv-remote")
local qr = nil
if options.show_qr then qr = require "qrencode" end

local function get_ip() 
    local openPop = io.popen('ip a | awk \'/inet / && !/127.0.0.1/ {split($2, a, "/"); print a[1]}\'', 'r')
    local output = openPop:read('*all')
    openPop:close()
    addrs = {}
    for a in output:gmatch("[^\n]+") do
        table.insert(addrs, a)
    end
    if next(addrs) == nil then
        error("could not retreive local address")
    end
    return addrs
end


local function qr_to_str(code)
    local s = ""
    local white_pixel = "\27[1;47m  \27[0m"
    local black_pixel = "\27[40m  \27[0m"
    for x=1,#code do
        for y=1,#code[x] do
            if code[x][y] > 0 then
                s = s .. white_pixel
            elseif code[x][y] < 0 then
                s = s .. black_pixel
            else
                s = s .."?"
            end
        end
        s = s .. "\n"
    end
    return s
end

local loop = cqueues.new()

local server = require("server").create(loop, options.host, options.port)

assert(server:listen())
do
    local ok, local_ip = pcall(get_ip)
    local bound_port = select(3, server._server:localname())
    if not ok then
        msg.warn("failed to retrieve addres of network device")
        print(string.format("Now listening on port %d", bound_port))
    else
        print(string.format("Now listening at %s:%d", local_ip[1], bound_port))
    end
    if options.show_qr and qr then
        local ok, code = qr.qrcode(string.format("http://%s:%d", local_ip[1], bound_port))
        if not ok then 
            msg.warn("failed to generate qr code for server address")
        else
            io.stdout:write(qr_to_str(code))
        end
    end
end

local running = true
mp.register_event("shutdown", function() running = false end)
while running do
    mp.dispatch_events()
    loop:step(0.5)
end
