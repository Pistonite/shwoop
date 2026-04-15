import { isHotReload, status, toast } from "./client/status_bar.ts";
import { ScrollTracker } from "./client/scroll.ts";
import { delayed, HOST, log } from "./client/util.ts";
import { startWebsocketSession } from "./client/websocket.ts";
import { getUrlWithRawQuery, handleLocationSwitchInIFrame } from "./client/location.ts";

let scrollTracker: ScrollTracker | undefined = undefined;
let prevFrame: HTMLIFrameElement | undefined = undefined;
let nextZIndex = 100;


const main = async () => {
    if (handleLocationSwitchInIFrame()) {
        return;
    }

    log("starting");
    status("", "initializing...");
    await hotReload(true);

    startWebsocketSession(`ws://${HOST}/`, async () => {
        if (!isHotReload()) {
            globalThis.location.reload();
            return;
        }
        log("start reload");
        stopCleanup();
        const startTime = performance.now();
        status("yellow", "reloading...");
        await hotReload(false);
        const elapsed = Math.floor(performance.now()-startTime);
        status("green", "connected", false);
        toast("green", `reloaded in ${elapsed}ms`);
    });
};

const CLEANUP_INTERVAL_MS = 5000;

let cleanupTimer: ReturnType<typeof setTimeout> | undefined = undefined;

const stopCleanup = () => {
    clearTimeout(cleanupTimer);
    cleanupTimer = undefined;
};

const scheduleCleanup = () => {
    stopCleanup();
    cleanupTimer = setTimeout(function tick() {
        const frame = document.querySelector(".content-frame-todelete");
        if (!frame) {
            cleanupTimer = undefined;
            return;
        }
        frame.remove();
        cleanupTimer = setTimeout(tick, CLEANUP_INTERVAL_MS);
    }, CLEANUP_INTERVAL_MS);
};

const hotReload = async (isFirst: boolean) => {
    const prevScrollTracker = scrollTracker;
    scrollTracker = undefined;
    prevScrollTracker?.stop();

    const frame = document.createElement("iframe");

    {
        const cancel = delayed(() => {
            if (!isFirst) {
                status("yellow", "reloading: fetching...");
            }
        });
        // load new frame
        frame.allow = "accelerometer; autoplay; bluetooth; camera; clipboard-read; clipboard-write; display-capture; encrypted-media; fullscreen; gamepad; geolocation; gyroscope; hid; identity-credentials-get; idle-detection; local-fonts; magnetometer; microphone; midi; payment; picture-in-picture; publickey-credentials-get; screen-wake-lock; serial; storage-access; usb; web-share; xr-spatial-tracking";
        frame.setAttribute("sandbox", "allow-downloads allow-forms allow-modals allow-orientation-lock allow-pointer-lock allow-popups allow-popups-to-escape-sandbox allow-presentation allow-same-origin allow-scripts allow-top-navigation allow-top-navigation-by-user-activation allow-top-navigation-to-custom-protocols");
        frame.className = "content-frame";

        const frameLoadedPromise = new Promise<void>(resolve => {
            frame.onload = () => resolve();
        });
        frame.src = getUrlWithRawQuery(globalThis.location);
        document.body.append(frame);

        await frameLoadedPromise;
        cancel();
    }

    // Copy title and favicon from frame into hosting document
    const frameDoc = frame.contentDocument;
    if (frameDoc) {
        const title = frameDoc.querySelector("title");
        if (title) {
            document.title = title.textContent ?? "";
        }
        // Remove all existing icons then insert the frame's icon if present
        document.querySelectorAll("link[rel~='icon']").forEach(el => el.remove());
        const icon = frameDoc.querySelector("link[rel~='icon']");
        if (icon) {
            document.head.append(icon.cloneNode());
        }
    }

    {
        const cancel = delayed(() => {
            if (!isFirst) {
                status("yellow", "reloading: restoring scroll...");
            }
        });

        const nextScrollTracker = new ScrollTracker(frame);
        await nextScrollTracker.start();
        prevScrollTracker?.apply(frame);
        cancel();
    }

    // put it on top
    frame.style.zIndex = `${nextZIndex++}`;
    if (prevFrame) {
        prevFrame.style.display = "none";
        prevFrame.classList.add("content-frame-todelete");
    }
    prevFrame = frame;

    scheduleCleanup();
}

void main();
