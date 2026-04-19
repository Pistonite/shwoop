import type { StatusBar } from "./status_bar.ts";
import { delayed, error, sleep } from "./util.ts";

const BEFORE_RECONNECT_S = 5;
const BACKOFF_BASE_MS = 3000;
const BACKOFF_MAX_MS = 30000;
const BACKOFF_GIVE_UP_MS = 10 * 60 * 1000; // 10 minutes

export const startWebsocketSession = (
    url: string,
    status: StatusBar,
    reload: (startTime: number) => void | Promise<void>,
) => {
    let nextBackoffMs = BACKOFF_BASE_MS;
    let totalWaitedMs = 0;
    let reloaded = false;
    let buildSucceess = true;
    let buildStartedTime = 0;
    const connect = (isFirst: boolean, isRetry: boolean) => {
        const cancel = delayed(() => {
            status.update("", "connecting...");
        });
        const ws = new WebSocket(url);

        ws.addEventListener("open", () => {
            cancel();
            if (!isFirst) {
                status.update("", "");
                reloaded = true;
                // reconnect case, hard reload the page since we are now back online
                globalThis.location.reload();
                return;
            }
            nextBackoffMs = BACKOFF_BASE_MS;
            totalWaitedMs = 0;
            status.update("green", "connected");
        });

        ws.addEventListener("message", (e) => {
            if (reloaded) {
                return;
            }
            if (typeof e.data !== "string") {
                error("unknown message", e.data);
                return;
            }
            switch (e.data) {
                case "reload": {
                    if (buildSucceess) {
                        const start = buildStartedTime;
                        buildStartedTime = 0;
                        void reload(start || performance.now());
                    }
                    return;
                }
                case "build-started": {
                    status.update("yellow", "building");
                    buildStartedTime = performance.now();
                    return;
                }
                case "build-failed": {
                    buildSucceess = false;
                    buildStartedTime = 0;
                    status.update("red", "build failed, please check server output");
                    status.updatePerfNumber("build", -1);
                    return;
                }
                case "build-succeeded": {
                    buildSucceess = true;
                    status.updatePerfNumber("build", performance.now() - buildStartedTime);
                    return;
                }
                default: {
                    error("unknown message", e.data);
                }
            }
        });

        ws.addEventListener("close", async () => {
            cancel();
            if (reloaded) {
                return;
            }
            // handle reconnect
            if (!isRetry) {
                for (let i = BEFORE_RECONNECT_S; i > 0; i--) {
                    status.update("red", `disconnected - reconnecting in ${i}s`);
                    await sleep(1000);
                }
            } else {
                const delay = nextBackoffMs;
                nextBackoffMs = Math.min(nextBackoffMs * 2, BACKOFF_MAX_MS);
                totalWaitedMs += delay;

                if (totalWaitedMs >= BACKOFF_GIVE_UP_MS) {
                    status.update("red", "refresh to restart hot-reload");
                    return;
                }

                const secs = Math.ceil(delay / 1000);
                for (let secsLeft = secs; secsLeft > 0; secsLeft--) {
                    if (secs > 5) {
                        status.update("yellow", `retrying in ${secsLeft}s (or refresh to reload)`);
                    } else {
                        status.update("yellow", `retrying in ${secsLeft}s`);
                    }
                    await sleep(1000);
                }
            }
            connect(false, true);
        });
    };
    connect(true, false);
};
