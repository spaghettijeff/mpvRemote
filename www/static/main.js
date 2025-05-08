function initTabs(doc) {
    const tabContainers = doc.querySelectorAll(".tab-container");

    function setupTabContainer(container) {
        const tabs = [...container.querySelectorAll(".tab")];
        const contents = [...container.querySelectorAll(".content")];
        container.addEventListener("click", createOnClick(tabs, contents));
        tabs[0].click();
    }
    function createOnClick(tabs, contents) {
        return (e) => {
            e.preventDefault();
            if (! e.target.classList.contains("tab")) return;
            const tabIdx = tabs.indexOf(e.target);
            for (const c of contents) {
                c.setAttribute("hidden", "");
                c.setAttribute("aria-selected", false);
            }
            for (const t of tabs) {
                t.setAttribute("aria-selected", false);
            }
            contents[tabIdx].removeAttribute("hidden");
            contents[tabIdx].setAttribute("aria-selected", true);
            tabs[tabIdx].setAttribute("aria-selected", true);
        };
    }
    for (const container of tabContainers) {
        setupTabContainer(container);
    }
}

class Signal {
    value;
    callbacks = [];
    constructor(value) {
        this.value = value;
        this.callbacks = [];
    }
    get () {
        return this.value;
    }
    set (v) {
        this.value = v;
        this.callbacks.forEach(it => it());
    }
    subscribe(callback) {
        this.callbacks.push(callback)
    }
}

var CallbackToBind = null;
function parseFunc(code) {
    return new Function(`return ${code};`);
}

function selectElements(parent) {
    const elements = parent.querySelectorAll('*')
    const taggedElements = {
        events: [],
        bindings: [],
    };
    for (const el of elements) {
        if([...el.attributes].find((attr) => attr.name.startsWith('@'))) taggedElements.events.push(el);
        if([...el.attributes].find((attr) => attr.name.startsWith('!'))) taggedElements.bindings.push(el);
    }
    return taggedElements;
}

function initBindings(elements) {
    for (const el of elements) {
        for (const attr of el.attributes) {
            let callback = null;
            switch(attr.name.substring(1).toLowerCase()) {
                case "innerhtml":
                    callback = new Function(`return ${attr.value};`)
                    CallbackToBind = () => { el.innerHTML = callback(); };
                    el.innerHTML = callback();
                    CallbackToBind = null;
                    break;
                case "value":
                    callback = new Function(`return ${attr.value};`)
                    CallbackToBind = () => { el.value = callback(); };
                    el.value = callback();
                    CallbackToBind = null;
                    break;
                case "show":
                    callback = new Function(`return ${attr.value};`)
                    CallbackToBind = () => { el.hidden = !callback(); };
                    el.hidden = !callback();
                    CallbackToBind = null;
                    break;
            }
        }
    }
}

function initEvents(elements) {
    for (const el of elements) {
        for (const attr of el.attributes) {
            if (attr.name.startsWith('@')) {
                const callback = new Function("e", `${attr.value};`)
                const event_type = attr.name.substring(1);
                el.addEventListener(event_type, callback);
            }
        }
    }
}

function init() {
    const elements = selectElements(document);
    initBindings(elements.bindings);
    initEvents(elements.events);
}

const initState = (obj) => {
    const state = new Proxy({}, {
        get(target, property) {
            if (CallbackToBind) target[property].subscribe(CallbackToBind);
            return target[property].get();
        },
        set(target, property, value) {
            if (target[property]) {
                target[property].set(value)
            } else {
                target[property] = new Signal(value);
            }
        }
    });
    for (let [key, val] of Object.entries(obj)) {
        state[key] = val;
    }
    return state;
};

const initSocket = () => {
    return {
        ws: null,
        reconnect_attempts: 0,
        max_attempts: 5,
        reconnect: function() {
            this.reconnect_attempts = 0;
            this.connect();
        },
        connect: function() {
            this.ws = new WebSocket("ws://" + window.location.host + "/socket");
            this.ws.addEventListener("open", (event) => {
                console.debug("socket connected: ", this.ws);
                ui["sock-conn"] = 1;
                this.send({event: "get-status"});
            });
            this.ws.addEventListener("close", (event) => {
                console.debug("socket closed: ", this.ws);
                if (this.reconnect_attempts < this.max_attempts) {
                    ui["sock-conn"] = 0;
                    setTimeout(() => {
                        this.connect();
                    }, 1000);
                    this.reconnect_attempts += 1;
                } else {
                    ui["sock-conn"] = -1;
                }
            });
            this.ws.addEventListener("message", (event) => {
                const packet = JSON.parse(event.data);
                console.debug("RECEIVED: ", packet);
                if (packet.event === "status") {
                    for (let [key, val] of Object.entries(packet.data)) {
                        state[key] = val;
                    }
                } else {
                    state[packet.event] = packet.data;
                }
            });
        },
        send: function(packet){
            console.debug("SENDING: ", packet)
            try { 
                this.ws.send(JSON.stringify(packet));
            } catch(err) {
                console.error(err);
            }
        }
    };
};

function renderReconnect(status) {
    switch(status) {
        case -1:
            return `
            <div class="flex items-center bg-gray-700 active:bg-gray-600 rounded-full">
                <span class="material-symbols-outlined text-red-600">
                error
                </span>
                <span class="text-xs font-semibold text-indigo-500 px-2">
                Re-Connect
                </span>
            </div>
            `;
        case 0:
            return `
            <div class="flex items-center bg-gray-800 rounded-full">
                <span class="material-symbols-outlined text-amber-600">
                pending
                </span>
                <span class="text-xs px-2">
                Connecting
                </span>
            </div>
            `;
        case 1:
            return "";
    }
}

function renderPlaylist(playlist) {
    if (!playlist) return "";
    let html = "";
    playlist.forEach((item, idx, array) => {
        let filename = item.filename.split('/').pop();
        html += `
            <li class="${item.current ? 'bg-green-800' : 'bg-white dark:bg-zinc-800'} shadow-md rounded-md my-4 p-2 flex content-center">
            <span class="material-symbols-outlined md-26"
        onclick="socket.send({event: 'playlist-move', data: [${idx}, ${idx <=0 ? array.length + ((idx-1) % array.length) : idx-1}]})">
            keyboard_arrow_up
            </span>
            <span class= px-2>
            ${filename}
            </span>
            <span class="material-symbols-outlined text-red-600 ml-auto md-26"
        onclick="socket.send({event: 'playlist-remove', data: ${idx}})">
            remove
            </span>
            </li>
            `;
    });
    return html;
}

async function renderDirectory(dir) {
    console.debug("Dir Picker: ", dir);
    const action = window.location.hash.substring(1);
    const resp = await fetch("/file-picker/" + dir);
    const directory = await resp.json();
    let html = `<ul class="font-lg divide-y divide-gray-200 dark:divide-gray-700">`;
    if (dir !== "") {
        html += `
            <li class="p-2" onclick="renderDirectory('${dir}'.substr(0, '${dir}'.lastIndexOf('\/')))">../</li>
            `;
    }
    for (d of directory.dirs.sort()) {
        if (dir === "") {
            html += `
                <li class="p-2" onclick="renderDirectory('${d}')">${d}</li>
                `;
        } else {
            html += `
                <li class="p-2" onclick="renderDirectory('${dir}/${d}')">${d}</li>
                `;
        }
    }
    for (f of directory.files.sort()) {
        html += `
            <li class="p-2"><a href="#" onclick="socket.send({event:'${action}', data:{ file: { dir: '${dir}', name: '${f}'}}})">${f}</a></li>
            `;
    }
    html += `</ul>`;
    document.querySelector("#file-browser").innerHTML = html;
}

function formatTime(seconds) {
    if (typeof(seconds) !== "number") return "--";
    seconds = Math.round(seconds);
    let hour = Math.floor(seconds / 3600);
    seconds %= 3600;
    let min = Math.floor(seconds / 60);
    let sec = Math.round(seconds % 60);
    let m = String(min).padStart(2, '0')
    let s = String(sec).padStart(2, '0')
    if (hour > 0) {
        return `${hour}:${m}:${s}`;
    } else {
        return `${min}:${s}`;
    }
}
const ui = initState({
    "file-picker": (window.location.hash === "#play-now") || (window.location.hash === "#playlist-add"),
    "sock-conn": -1,
})
window.addEventListener("hashchange", () => {
    ui["file-picker"] = (window.location.hash === "#play-now") || (window.location.hash === "#playlist-add");
});
const state = initState({
    "duration" : null,
    "fullscreen" : null,
    "media-title" : null,
    "pause" : null,
    "playlist" : null,
    "time-pos" : null,
    "core-idle" : null,
    "volume" : null,
});
var timer = null;
CallbackToBind = () => {
    if (state["pause"] === false && state["core-idle"] === false && timer === null) {
        timer = setInterval(() => {
            state["time-pos"] += 0.1;
        }, 100)
    } else {
        clearInterval(timer)
        timer = null;
    }
};
state["pause"];
state["core-idle"]
CallbackToBind = null;

initTabs(document);
const socket = initSocket();
socket.connect();
init();
