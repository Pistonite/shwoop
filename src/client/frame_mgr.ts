import { CLASSNAME, Frame } from "./frame.ts";
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
            // if there aren't any frames yet, the frame we are about to show
            // will appear immediately. this will cause flickering since
            // the CSS are still being applied. usually we load the frame
            // behind an existing frame. in case there is none, then
            // we use a hack to set opacity to very low so it's barely visible
            // (setting display:none might cause browser to de-prioritize loading it)
            //
            // this will only last for a few hundred milliseconds so it's barely noticable
            const hasExistingFrame = !!document.querySelector(`iframe.${CLASSNAME}`);
            if (!hasExistingFrame) {
                (frame.element() as HTMLIFrameElement).style.opacity = "0.01";
            }
            await frame.show(this.status, this.currentPathname);
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
            (frame.element() as HTMLIFrameElement).style.opacity = "";
            this.active = frame;
            // clean document needs to be async. otherwise pushing
            // the frame to top and removing the other nodes at the same time
            // will cause flicker
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

export interface FrameTask {
    pathname: string;
    resolve: () => void;
}
