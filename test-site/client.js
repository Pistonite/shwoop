const dot = document.getElementById("dot");
const statusText = document.getElementById("status-text");
const logEl = document.getElementById("log");
const reloadBtn = document.getElementById("btn-reload");

function log(msg, cls = "info") {
  const entry = document.createElement("div");
  entry.className = `log-entry ${cls}`;
  entry.textContent = `[${new Date().toISOString()}] ${msg}`;
  logEl.appendChild(entry);
  logEl.scrollTop = logEl.scrollHeight;
}

function setStatus(connected) {
  dot.className = `dot ${connected ? "connected" : "disconnected"}`;
  statusText.textContent = connected ? "connected" : "disconnected";
}

const WS_URL = `ws://${location.host}/`;
const RECONNECT_DELAY_MS = 2000;

function connect() {
  log(`connecting to ${WS_URL}`);
  const ws = new WebSocket(WS_URL);

  ws.addEventListener("open", () => {
    setStatus(true);
    log("connection opened");
  });

  ws.addEventListener("message", (e) => {
    if (e.data === "reload") {
      log("received reload", "reload");
    } else {
      log(`received: ${e.data}`);
    }
  });

  ws.addEventListener("close", (e) => {
    setStatus(false);
    log(`connection closed (code ${e.code})`, "error");
    setTimeout(connect, RECONNECT_DELAY_MS);
  });

  ws.addEventListener("error", () => {
    log("websocket error", "error");
  });
}

reloadBtn.addEventListener("click", async () => {
  try {
    const res = await fetch("/reload", { method: "POST" });
    log(`POST /reload → ${res.status}`);
  } catch (e) {
    log(`POST /reload failed: ${e}`, "error");
  }
});

connect();
