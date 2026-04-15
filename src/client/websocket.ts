import { status } from "./status_bar.ts";
import { error, sleep } from "./util.ts";

const BEFORE_RECONNECT_S = 5;
const BACKOFF_BASE_MS = 3000;
const BACKOFF_MAX_MS = 30000;
const BACKOFF_GIVE_UP_MS = 10 * 60 * 1000; // 10 minutes

export const startWebsocketSession = (url: string, reload: () => void | Promise<void>) => {
    let nextBackoffMs = BACKOFF_BASE_MS;
    let totalWaitedMs = 0;
    let reloaded = false;
    const connect = (isFirst: boolean, isRetry: boolean) => {
        status("", "connecting...");
        const ws = new WebSocket(url);

        ws.addEventListener("open", () => {
            if (!isFirst) {
                status("", "");
                reloaded = true;
                // reconnect case, hard reload the page since we are now back online
                globalThis.location.reload();
                return;
            }
            nextBackoffMs = BACKOFF_BASE_MS;
            totalWaitedMs = 0;
            status("green", "connected");
        });

        ws.addEventListener("message", (e) => {
            if (reloaded) {
                return;
            }
            if (e.data === "reload") {
                void reload();
                return;
            }
            error(BIN_NAME, "unknown message", e.data);
        });

        ws.addEventListener("close", async () => {
            if (reloaded) {
                return;
            }
            // handle reconnect
            if (!isRetry) {
                for (let i = BEFORE_RECONNECT_S; i > 0; i--) {
                    status("red", `disconnected - reconnecting in ${i}s`);
                    await sleep(1000);
                }
            } else {
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
            }
            connect(false, true);
        });
    };
    connect(true, false);
};
