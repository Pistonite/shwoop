import { StatusBar } from "./client/status_bar.ts";
import { StateTracker } from "./client/tracker.ts";
import { HOST, log } from "./client/util.ts";
import { startWebsocketSession } from "./client/websocket.ts";
import { Frame } from "./client/frame.ts";
import { FrameMgr } from "./client/frame_mgr.ts";

const main = async () => {
    const initialPathState = {
        pathname: location.pathname,
        search: location.search,
        hash: location.hash,
    };
    const initialPathname = location.pathname + location.search;
    // initialize the status bar
    const status = new StatusBar();
    status.update("", "initializing...");
    let tracker: StateTracker | undefined;
    if (status.isStateTrackingEnabled()) {
        // enable tracking on the top document
        const t = new StateTracker(status, Frame.thisDocument(), undefined);
        if (await t.start()) {
            tracker = t;
        }
    }
    // add popstate event for handling navigation within the frame
    window.addEventListener("popstate", async (e) => {
        log("popping state to: " + location.pathname + location.search);
        log("got state:" + JSON.stringify(e.state));
        log("(initial state):" + JSON.stringify(initialPathState));
        // if (!isHotReload()) {
        //     setTimeout(() => {
        //         globalThis.location.reload();
        //     }, 100);
        // }
        // const pathname = e.state.pathname;
        // if (pathname && typeof pathname === "string") {
        //     const frames = document.querySelectorAll<HTMLIFrameElement>("iframe.content-frame");
        //     const len = frames.length;
        //     for (let i =0;i<len;i++) {
        //         frames[i].style.zIndex= "1";
        //     }
        //     // log("hot reloading: "+pathname);
        //     // thePathname = pathname;
        //     // await hotReload(false);
        // }
    });

    const mgr = new FrameMgr(status, initialPathname, tracker);

    startWebsocketSession(`ws://${HOST}/`, status, async () => {
        if (!status.isHotReloadEnabled()) {
            location.reload();
            return;
        }
        const startTime = performance.now();
        await mgr.reload();
        // status("yellow", "reloading...");
        // await hotReload(false);
        const elapsed = Math.floor(performance.now() - startTime);
        status.updateBaseOnly("green", "connected");
        status.toast("green", `reloaded in ${elapsed}ms`);
    });
};

// const hotReload = async (isFirst: boolean) => {
//     while (isReloading) {
//         await sleep(10);
//     }
//     isReloading = true;
//     const prevTracker = stateTracker;
//     stateTracker = undefined;
//     prevTracker?.stop();
//
//     const frame = document.createElement("iframe");
//
//     // if (!isFirst) {
// {
//         const cancel = delayed(() => {
//             if (!isFirst) {
//                 status("yellow", "reloading: fetching...");
//             }
//         });
//         // load new frame
//         frame.className = "content-frame";
//         frame.style.zIndex = "10";
//
//         const frameLoadedPromise = new Promise<void>((resolve) => {
//             frame.onload = () => resolve();
//         });
//         frame.dataset.loadpath = thePathname;
//         frame.src = getUrlWithRawQuery({
//             pathname: thePathname,
//             search: globalThis.location.search,
//             hash: globalThis.location.hash,
//         });
//         log("loading new frame: "+frame.src);
//         document.body.append(frame);
//
//         await frameLoadedPromise;
//         if (isFirst) {
//             status("", `rendered in ${Math.floor(performance.now())}ms`)
//         }
//         cancel();
//     }
//
//     // Copy title and favicon from frame into hosting document
//     const frameDoc = frame.contentDocument;
//     if (frameDoc) {
//         const title = frameDoc.querySelector("title");
//         if (title) {
//             document.title = title.textContent ?? "";
//         }
//         // Remove all existing icons then insert the frame's icon if present
//         document.querySelectorAll("link[rel~='icon']").forEach((el) => el.remove());
//         const icon = frameDoc.querySelector("link[rel~='icon']");
//         if (icon) {
//             document.head.append(icon.cloneNode());
//         }
//     }
//
//     if (isStateRestoreEnabled()) {
//         const cancel = delayed(() => {
//             if (!isFirst) {
//                 status("yellow", "reloading: restoring state...");
//             }
//         });
//
//         if (thePathname === prevTracker?.pathname) {
//             const nextTracker = new StateTracker(thePathname, frame, prevTracker?.getLastStabilizationTimeMs());
//             await nextTracker.start();
//             prevTracker?.apply(frame);
//             stateTracker = nextTracker;
//         } else {
//             const nextTracker = new StateTracker(thePathname, frame, undefined);
//             await nextTracker.start();
//             stateTracker = nextTracker;
//         }
//
//         cancel();
//     }
//
//     // put it on top
//     frame.style.zIndex = `${nextZIndex++}`;
//     if (prevFrame) {
//         prevFrame.style.display = "none";
//         prevFrame.classList.add("content-frame-todelete");
//     }
//     prevFrame = frame;
//
//     scheduleCleanup();
//     isReloading = false;
// };

void main();
