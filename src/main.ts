import { status } from "./client/status_bar.ts";
import { HOST, log } from "./client/util.ts";
import { startWebsocketSession } from "./client/websocket.ts";

const path = globalThis.location.pathname;

log("foo");

const createIframe= (): HTMLIFrameElement => {
    const iframe = document.createElement("iframe");
    iframe.allow = "accelerometer; autoplay; bluetooth; camera; clipboard-read; clipboard-write; display-capture; encrypted-media; fullscreen; gamepad; geolocation; gyroscope; hid; identity-credentials-get; idle-detection; local-fonts; magnetometer; microphone; midi; payment; picture-in-picture; publickey-credentials-get; screen-wake-lock; serial; storage-access; usb; web-share; xr-spatial-tracking";
    iframe.setAttribute("sandbox", "allow-downloads allow-forms allow-modals allow-orientation-lock allow-pointer-lock allow-popups allow-popups-to-escape-sandbox allow-presentation allow-same-origin allow-scripts allow-top-navigation allow-top-navigation-by-user-activation allow-top-navigation-to-custom-protocols");
    iframe.className = "content-frame";
    return iframe;
}

const frame = createIframe();
frame.onload = () => {
    const frameWindow = frame.contentWindow;
    if (!frameWindow) {
        return;
    }
    frameWindow.addEventListener("scroll", (e) => {
        console.log(e);
    })
}
frame.src = `${path}?x-shwoop-is-raw=1`;
document.body.append(frame);

startWebsocketSession(`ws://${HOST}/`, () => {
    console.log("received reload")
});
