import { status } from "./status_bar.ts";
import { error, sleep } from "./util.ts";

const BEFORE_RECONNECT_S = 5;
const BACKOFF_BASE_MS = 3000;
const BACKOFF_MAX_MS = 30000;
const BACKOFF_GIVE_UP_MS = 10 * 60 * 1000; // 10 minutes

export const startWebsocketSession= (
    url: string,
    reload: () => void | Promise<void>
) => {
    let nextBackoffMs = BACKOFF_BASE_MS;
    let totalWaitedMs = 0;
    const connect = () => {
        status("", "connecting...");
        const ws = new WebSocket(url);
        let isConnected = false;

        ws.addEventListener("open", () => {
            isConnected = true;
            nextBackoffMs = BACKOFF_BASE_MS;
            totalWaitedMs = 0;
            status("green", "connected");
        });

        ws.addEventListener("message", (e) => {
            if (e.data === "reload") {
                void reload();
                return;
            }
            error(BIN_NAME,"unknown message",e.data);
        });

        ws.addEventListener("close", async () => {
            if (isConnected) {
                for (let i = BEFORE_RECONNECT_S; i > 0; i--) {
                    status("red", `disconnected - reconnecting in ${i}s`);
                    await sleep(1000);
                }
            }

            // handle reconnect
            
            const delay = nextBackoffMs;
            nextBackoffMs = Math.min(nextBackoffMs * 2, BACKOFF_MAX_MS);
            totalWaitedMs += delay;

            if (totalWaitedMs >= BACKOFF_GIVE_UP_MS) {
                status("red", "refresh to restart hot-reload");
                return;
            }

            const secs = Math.ceil(delay / 1000);
            for (let secsLeft = secs; secsLeft > 0; secsLeft--) {
                if (secs > 5) {
                    status("yellow", `retrying in ${secsLeft}s (or refresh to reload)`);
                } else {
                    status("yellow", `retrying in ${secsLeft}s`);
                }
                await sleep(1000);
            }
            connect();
        });
    };
    connect();
}

