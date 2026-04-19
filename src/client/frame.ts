import type { StatusBar } from "./status_bar.ts";
import { error, type Class } from "./util.ts";

export const CLASSNAME = BIN_NAME + "--content-frame";

export class Frame {
    public static thisDocument(): Frame {
        return new Frame(undefined);
    }
    public static newIFrame(zIndex: string): Frame {
        const frame = document.createElement("iframe");
        frame.className = CLASSNAME;
        frame.style.zIndex = zIndex;

        return new Frame(frame);
    }
    private constructor(
        // undefine means proxy to the current `document`
        private inner: HTMLIFrameElement | undefined,
    ) {}

    public element(): HTMLIFrameElement | undefined {
        return this.inner;
    }

    public setZIndex(zIndex: string) {
        if (this.inner) {
            this.inner.style.zIndex = zIndex;
        }
    }

    /** Add the frame to the document (if not already) and display the URL */
    public async show(status: StatusBar, url: string): Promise<void> {
        if (!this.inner) {
            location.href = url;
            return;
        }
        const stop = status.startPerfMeasure("page-load");
        const frameLoadedPromise = new Promise<void>((resolve) => {
            (this.inner as HTMLIFrameElement).onload = () => resolve();
        });
        this.inner.src = url;
        document.body.append(this.inner);
        await frameLoadedPromise;
        stop();
    }

    public remove() {
        this.inner?.remove();
    }

    /** Cast an element inside the frame, return null if e is not the type */
    public cast<T extends Element>(e: Element | null, clazz: Class<T>): T | null {
        const frame = this.inner;
        if (!frame) {
            return e instanceof clazz ? e : null;
        }
        if (!e) {
            return null;
        }
        if (e instanceof clazz) {
            return e;
        }
        const name = clazz.name;
        try {
            const iframeWindow = frame.contentWindow;
            if (!iframeWindow) {
                return null;
            }
            if (!(name in iframeWindow)) {
                return null;
            }
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            const iframeClass = (iframeWindow as any)[name];
            if (typeof iframeClass !== "function") {
                return null;
            }
            if (e instanceof iframeClass) {
                return e as T;
            }
        } catch {
            // fallthrough
        }
        return null;
    }

    /** Get the document of the frame */
    public document(): Document | null {
        if (!this.inner) {
            return document;
        }
        let d: Document | null = null;
        try {
            d = this.inner.contentDocument;
        } catch (e) {
            error(e);
        }
        if (d) {
            return d;
        }
        error("failed to access iframe document, is it somehow cross-origin?");
        return null;
    }

    /** Get the window of the frame */
    public window(): Window | null {
        if (!this.inner) {
            return window;
        }
        let d: Window | null = null;
        try {
            d = this.inner.contentWindow;
        } catch (e) {
            error(e);
        }
        if (d) {
            return d;
        }
        error("failed to access iframe window, is it somehow cross-origin?");
        return null;
    }
}
