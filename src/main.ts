import { StatusBar } from "./client/status_bar.ts";
import { StateTracker } from "./client/tracker.ts";
import { startWebsocketSession } from "./client/websocket.ts";
import { Frame } from "./client/frame.ts";
import { FrameMgr } from "./client/frame_mgr.ts";

const main = async () => {
    const initialPathname = location.pathname + location.search + location.hash;
    // initialize the status bar
    const status = new StatusBar();

    const frameManagerPromise = (async () => {
        let tracker: StateTracker | undefined;
        if (status.isStateTrackingEnabled()) {
            // enable tracking on the top document
            const t = new StateTracker(status, Frame.thisDocument(), undefined);
            if (await t.start()) {
                tracker = t;
            }
        }

        return new FrameMgr(status, initialPathname, tracker);
    })();

    startWebsocketSession(`ws://${location.host}/`, status, async () => {
        if (!status.isHotReloadEnabled()) {
            location.reload();
            return;
        }
        const startTime = performance.now();
        const ok = await (await frameManagerPromise).reload();
        if (!ok) {
            return;
        }
        const elapsed = Math.floor(performance.now() - startTime);
        status.updateBaseOnly("green", "connected");
        status.toast("green", `reloaded in ${elapsed}ms`);
    });

    status.updatePerfNumber("page-load", performance.now());
};

void main();
