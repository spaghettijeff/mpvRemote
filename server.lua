local http_server = require "http.server"
local http_headers = require "http.headers"
local http_websocket = require "http.websocket"
local utils = require "mp.utils"
local msg = require "mp.msg"

local remote = require "remote"

local M = {}

M.create = function(cqueue, host, port)
    local parse_url = function(url) 
        local result = {}
        for w in url:gmatch("([^/]+)/?") do
            result[#result + 1] = w
        end
        return result
    end

    local mime_types = {
        ["text"] = "text/plaintext";
        ["html"] = "text/html";
        ["css"] = "text/css";
        ["js"] = "text/javascript";
        ["json"] = "application/json";
        ["svg"] = "image/svg+xml";
        ["woff2"] = "font/woff2";
    };

    local routes = {
        ["index"] = function(request_headers, stream, path_tail) 
            local response_headers = http_headers.new()
            local page = io.open(mp.get_script_directory() .. "/www/index.html", "r")
            response_headers:append(":status", "200")
            response_headers:append("Content-Type", "text/html")
            assert(stream:write_headers(response_headers, false), "failed to write response headers")
            assert(stream:write_body_from_file(page))
        end;
        ["socket"] = function(request_headers, stream, path_tail)
            if request_headers:get("upgrade") == "websocket" then
                local ws = http_websocket.new_from_stream(stream, request_headers)
                ws:accept({}, 5)
                remote.listen(ws)
            end
        end;
        ["static"] = function(request_headers, stream, path_tail)
            local response_headers = http_headers.new()
            local fpath = "www/static/"..path_tail
            local _, _, ftype = string.find(path_tail, "%.([^%.]+)$")
            local page = io.open(mp.get_script_directory() .. fpath, "r")
            response_headers:append(":status", "200")
            response_headers:append("Content-Type", mime_types[ftype])
            assert(stream:write_headers(response_headers, false))
            assert(stream:write_body_from_file(page))
        end;
        ["file-picker"] = function(request_header, stream, path_tail)
            local response_headers = http_headers.new()
            dirs = utils.readdir(utils.getcwd() .. '/'.. (path_tail or ""), "dirs")
            files = utils.readdir(utils.getcwd() .. '/' .. (path_tail or ""), "files")
            response_headers:append(":status", "200")
            response_headers:append("Content-Type", "application/json")
            assert(stream:write_headers(res_headers, false))
            assert(stream:write_body_from_string(utils.format_json{files = files or {}, dirs = dirs or {}}))
        end;
    };
    local onstream = function(server, stream) 
        local request_headers = assert(stream:get_headers(), "failed to retrieve request headers")
        msg.debug(string.format('"%s %s HTTP/%g"  "%s" "%s"',
        request_headers:get(":method") or "",
        request_headers:get(":path") or "",
        stream.connection.version,
        request_headers:get("referer") or "-",
        request_headers:get("user-agent") or "-"))

        local raw_path = request_headers:get(":path")
        local _, _, path_root, path_tail = string.find(raw_path, "([^/]+)/?(.*)$")
        local route = routes[path_root or "index"]
        assert(route, raw_path.." is not a valid route")
        route(request_headers, stream, path_tail)
    end;

    local onerror = function(myserver, context, op, err, errno) 
        local message = op .. " on " .. utils.to_string(context) .. " failed"
        if err then
            message = message .. ": " .. utils.to_string(err)
        end
        msg.error(message)
    end;
    return {
        _server = http_server.listen {
            cq = cqueue;
            host = host;
            port = port;
            onstream = onstream;
            onerror = onerror;
        };
        listen = function(self)
            return self._server:listen()
        end;
    }
end

return M
