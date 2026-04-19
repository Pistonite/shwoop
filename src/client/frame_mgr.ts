import { Frame, CLASSNAME } from "./frame.ts";
import type { StatusBar } from "./status_bar.ts";
import { StateTracker } from "./tracker.ts";

export class FrameMgr {
    private active: Frame | undefined;
    private reloadingPromise: Promise<boolean> | undefined;
    private reloadingScheduled: boolean = false;
    private nextZIndex: number = 100;

    constructor(
        private status: StatusBar,
        private currentPathname: string,
        private tracker: StateTracker | undefined,
    ) {}

    /** Immediately show the pathname in the frame */
    public async switchTo(pathname: string): Promise<boolean> {
        const frame = Frame.newIFrame(`${this.nextZIndex++}`);
        const existingFrames = document.querySelectorAll<HTMLIFrameElement>(`iframe.${CLASSNAME}`);
        const length = existingFrames.length;
        for (let i = 0; i < length; i++) {
            const f = existingFrames[i];
            // bring existingFrames to lower z level than default
            f.style.zIndex = f === this.active?.element() ? "2" : "1";
        }
        // show the new frame
        this.active = frame;
        this.currentPathname = pathname;
        await frame.show(pathname);
        if (this.active !== frame || this.currentPathname !== pathname) {
            // no longer current after the frame got shown
            frame.remove();
            return false;
        }
        this.copyNodesFromActiveFrame();
        this.tracker?.stop();
        this.tracker = undefined;
        if (this.status.isStateTrackingEnabled()) {
            // restart tracking the new frame
            const tracker = new StateTracker(this.status, frame, undefined);
            const ok = await tracker.start();
            if (this.active !== frame || this.currentPathname !== pathname) {
                // no longer current after the frame starts being tracked
                tracker.stop();
                frame.remove();
                return false;
            }
            if (ok) {
                this.tracker = tracker;
            }
        }
        this.cleanDocumentSync();
        return true;
    }
    /** Prepare the page in the background and when ready, bring the frame to the top */
    public async reload(): Promise<boolean> {
        this.reloadingScheduled = true;
        if (this.reloadingPromise) {
            return this.reloadingPromise;
        }
        this.reloadingPromise = this.reloadInternal().then();
        const result = await this.reloadingPromise;
        this.reloadingPromise = undefined;
        return result;
    }

    /** Prepare the page in the background and when ready, bring the frame to the top */
    private async reloadInternal(): Promise<boolean> {
        while (this.reloadingScheduled) {
            this.reloadingScheduled = false;
            const frame = Frame.newIFrame("10");
            const pathname = this.currentPathname;
            await frame.show(this.currentPathname);
            // did we switch path during this time?
            if (this.currentPathname !== pathname) {
                return false;
            }
            if (this.reloadingScheduled) {
                continue;
            }
            this.copyNodesFromActiveFrame();
            if (this.status.isStateTrackingEnabled()) {
                const tracker = new StateTracker(this.status, frame, this.tracker);
                const ok = await tracker.start();
                if (this.currentPathname !== pathname) {
                    tracker.stop();
                    return false;
                }
                if (this.reloadingScheduled) {
                    tracker.stop();
                    continue;
                }
                // apply old tracker state
                this.tracker?.stop();
                await this.tracker?.apply(frame);
                if (this.currentPathname !== pathname) {
                    tracker.stop();
                    return false;
                }
                if (this.reloadingScheduled) {
                    tracker.stop();
                    continue;
                }
                if (ok) {
                    this.tracker = tracker;
                } else {
                    this.tracker = undefined;
                }
            }
            // all ready, bring the frame to top
            frame.setZIndex(`${this.nextZIndex++}`);
            this.active = frame;
            this.cleanDocument();
        }
        return true;
    }

    private copyNodesFromActiveFrame() {
        const doc = this.active?.document();
        if (!doc) {
            return;
        }
        const title = doc.querySelector("title");
        if (title) {
            document.title = title.textContent ?? "";
        }
        // Remove all existing icons then insert the frame's icon if present
        const icon = doc.querySelector("link[rel~='icon']");
        if (icon) {
            const oldIcons = Array.from(document.querySelectorAll("link[rel~='icon']"));
            document.head.append(icon.cloneNode());
            oldIcons.forEach((e) => e.remove());
        }
    }

    private cleanDocument() {
        setTimeout(() => this.cleanDocumentSync(), 1000);
    }

    private cleanDocumentSync() {
        const toRemove = [];
        {
            const children = document.body.children;
            const length = children.length;
            for (let i = 0; i < length; i++) {
                const e = children[i];
                if (e === this.status.element() || e === this.active?.element()) {
                    continue;
                }
                toRemove.push(e);
            }
        }
        {
            const children = document.head.children;
            const length = children.length;
            for (let i = 0; i < length; i++) {
                const e = children[i];
                if (!(e instanceof HTMLElement)) {
                    continue;
                }
                if (e instanceof HTMLMetaElement || e instanceof HTMLTitleElement) {
                    continue;
                }
                if (e.dataset.isshwoop === "true") {
                    continue;
                }
                toRemove.push(e);
            }
        }
        toRemove.forEach((e) => e.remove());
    }
}

export type FrameTask = {
    pathname: string;
    resolve: () => void;
};
