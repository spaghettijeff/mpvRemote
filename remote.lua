local utils = require "mp.utils"
local msg = require "mp.msg"

ConnectedSockets = {}

local function send(packet, ws) 
    local json_packet = utils.format_json(packet)
    if ws ~= nil then
        msg.debug("sending packet: ", utils.to_string(packet), "to: ", utils.to_string(ws))
        ws:send(json_packet)
    else
        msg.debug("sending packet: ", utils.to_string(packet), "to all: ", utils.to_string(ConnectedSockets))
        for _, ws in pairs(ConnectedSockets) do
            ws:send(json_packet)
        end
    end
end

local function get_mpv_status()
    local values = {
        duration = mp.get_property_native("duration") or '',
        ["media-title"] = mp.get_property('media-title') or '',
        fullscreen = mp.get_property_native("fullscreen"),
        pause = mp.get_property_native("pause"),
        playlist = mp.get_property_native("playlist") or {},
        ["time-pos"] = mp.get_property_native("time-pos") or '',
        volume = mp.get_property_native("ao-volume") or '',
    }
    return { event = "status", data = values }
end

--setup observing values
local function send_evt(name, value)
    if value ~= nil then
        send{event = name, data = value}
    end
end
do
    mp.observe_property("time-pos", "number", send_evt)
    mp.observe_property("pause", "bool", send_evt)
    mp.observe_property("fullscreen", "bool", send_evt)
    mp.observe_property("file-loaded", nil, function() send(get_mpv_status()) end)
    mp.observe_property("playlist", nil, function() send{event = 'playlist', data = mp.get_property_native('playlist')} end)
end


-- client event handlers
local api_endpoints = {
    ["toggle-play"] = function() 
        local paused = mp.get_property_bool("pause")
        mp.set_property_bool("pause", not paused)
    end,
    ["toggle-fullscreen"] = function() 
        local fullscreen = mp.get_property_bool("fullscreen")
        mp.set_property_bool("fullscreen", not fullscreen)
    end,
    ["volume"] = function(packet, ws)
        mp.set_property_number("ao-volume", packet.data)
        send{event="volume", data=packet.data}
    end,
    ["get-status"] = function(packet, ws) 
        local status = get_mpv_status()
        send(status, ws)
    end,
    ["seek"] = function(packet, ws)
        if packet.data.absolute then
            mp.commandv("seek", packet.data.absolute, "absolute")
        elseif packet.data.relative then
            mp.commandv("seek", packet.data.relative, "relative")
        end
    end,
    ["skip"] = function(packet, ws)
            mp.commandv("playlist-" .. packet.data)
    end,
    ["play-now"] = function(packet, ws) 
        local file
        if packet.data.url then
            file = packet.data.url
        elseif packet.data.file then
            file = utils.getcwd() .. utils.join_path(packet.data.file.dir, packet.data.file.name)
        end
        mp.commandv("loadfile", file, "replace")
    end,
    ["playlist-add"] = function(packet, ws) 
        local file
        if packet.data.url then
            file = packet.data.url
        elseif packet.data.file then
            file = utils.getcwd() .. utils.join_path(packet.data.file.dir, packet.data.file.name)
        end
        mp.commandv("loadfile", file, "append-play")
    end,
    ["playlist-remove"] = function(packet, ws)
        mp.commandv("playlist-remove", packet.data)
    end,
    ["playlist-move"] = function(packet, ws)
        if packet.data[1] < packet.data[2] then
            packet.data[2] = packet.data[2] + 1
        end
        mp.commandv("playlist-move", packet.data[1], packet.data[2])
    end,
    ["shutdown"] = function(packet, ws) 
        send{event = "shutdown"}
        mp.commandv("quit")
    end,
    ["stop"] = function(packet, ws) 
        mp.commandv("write-watch-later-config")
        mp.commandv("stop")
    end,
}
local function client_evt_router(packet, ws)
    local dispatcher = api_endpoints[packet.event]
    if dispatcher then
        dispatcher(packet, ws)
    end
end

local function receive(packet, ws)
    client_evt_router(utils.parse_json(packet), ws)
end

return {
    listen = function(ws) 
        ConnectedSockets[ws.key] = ws
        for p in ws:each() do
            receive(p, ws)
        end
        ConnectedSockets[ws.key] = nil
    end,
}
