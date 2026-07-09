import type { Frame } from "./frame.ts";
import type { StatusBar } from "./status_bar.ts";
import { error, log, sleep } from "./util.ts";

interface ScrollEntry {
    id: string;
    /** tag stack from body (e.g. div>div>p) */
    tag: string;
    scrollTop: number;
    scrollLeft: number;
}

interface StackItem {
    /** The element to process. */
    elem: HTMLElement;
    /** Full path to this element **at the time of stabilization** */
    path: string;
    /** tag stack from body, separated by > */
    tag: string;
}

const BATCH_TARGET_MS = 50;
const BATCH_MAX_SIZE = 10000;
const STABILIZE_DEBOUNCE_MS = 1000; // start with 1000 ms
const STABILIZE_DEBOUNCE_MS_MIN = 20; // wait for at least this ms for the document to stablize and any css/dom change to be done
const STABILIZE_TIMEOUT_MS = 2000;

/** Tracks state of the document that can be re-applied when reloading */
export class StateTracker {
    private controller: AbortController;
    private state: Map<string, ScrollEntry>;
    private changedWhileStarting: boolean;
    private lastStabilizationMs: number | undefined;
    private enabled: boolean;

    constructor(
        private status: StatusBar,
        // public pathname: string,
        private rootFrame: Frame,
        previous: StateTracker | undefined,
    ) {
        this.enabled = this.status.isStateTrackingEnabled();
        this.controller = new AbortController();
        this.state = new Map();
        this.changedWhileStarting = false;
        this.lastStabilizationMs = previous?.lastStabilizationMs;
    }

    public stop() {
        this.controller.abort();
    }

    public getLastStabilizationTimeMs(): number | undefined {
        return this.lastStabilizationMs;
    }

    public async start(): Promise<boolean> {
        const rootBody = this.rootFrame.document()?.body;
        if (!rootBody) {
            this.status.toast("red", "failed to get body of root frame, state will not be tracked");
            // attempt to wait for at least some time for the render to be ready
            await sleep(STABILIZE_DEBOUNCE_MS_MIN);
            this.status.updatePerfNumber("stabilize", STABILIZE_DEBOUNCE_MS_MIN);
            return false;
        }
        {
            const stop = this.status.startPerfMeasure("stabilize");
            const stabilizationDebounceMs = this.lastStabilizationMs
                ? Math.max(this.lastStabilizationMs * 2, STABILIZE_DEBOUNCE_MS_MIN)
                : STABILIZE_DEBOUNCE_MS;
            if (!(await this.waitForStabilization(rootBody, stabilizationDebounceMs))) {
                this.status.toast(
                    "red",
                    `the document structure did not stablize after ${stabilizationDebounceMs}, state will not be tracked`,
                );
                this.status.updatePerfNumber("stabilize", -1);
                return false;
            }
            stop();
        }
        const mutationWhileAddingElementObserver = new MutationObserver(() => {
            this.changedWhileStarting = true;
            mutationWhileAddingElementObserver.disconnect();
        });
        mutationWhileAddingElementObserver.observe(rootBody, { childList: true, subtree: true });
        this.controller.signal.addEventListener(
            "abort",
            () => mutationWhileAddingElementObserver.disconnect(),
            { once: true },
        );

        if (!this.enabled) {
            this.status.updatePerfNumber("track", -1);
            return false;
        }

        const stop = this.status.startPerfMeasure("track");

        let elemCount = 0;
        this.rootFrame.window()?.addEventListener(
            "scroll",
            () => {
                const rect = this.rootFrame.document()?.body?.getBoundingClientRect();
                const sTop = rect?.top || 0;
                const sLeft = rect?.left || 0;
                if (sTop || sLeft) {
                    this.state.set("", {
                        id: "",
                        tag: "",
                        scrollTop: -sTop,
                        scrollLeft: -sLeft,
                    });
                } else {
                    this.state.delete("");
                }
            },
            { passive: true, signal: this.controller.signal },
        );

        try {
            const stack: StackItem[] = [];
            const bodyChildren = rootBody.children;
            for (let i = bodyChildren.length - 1; i >= 0; i--) {
                const e = this.rootFrame.cast(bodyChildren[i], HTMLElement);
                if (!e) {
                    continue;
                }
                stack.push({ elem: e, path: `${i}`, tag: e.tagName.toLowerCase() });
            }

            let batchSize = 10;
            while (stack.length > 0) {
                const batchStartTime = performance.now();
                let processed = 0;

                while (stack.length > 0 && processed < batchSize) {
                    // stack length checked above
                    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
                    const { elem, path, tag } = stack.pop()!;
                    // process this element
                    if (isScrollable(elem)) {
                        elemCount++;
                        elem.addEventListener(
                            "scroll",
                            () => {
                                const sTop = elem.scrollTop;
                                const sLeft = elem.scrollLeft;
                                if (sTop || sLeft) {
                                    this.state.set(path, {
                                        id: elem.id,
                                        tag,
                                        scrollTop: sTop,
                                        scrollLeft: sLeft,
                                    });
                                } else {
                                    this.state.delete(path);
                                }
                            },
                            { passive: true, signal: this.controller.signal },
                        );
                    }

                    if (this.rootFrame.cast(elem, HTMLIFrameElement)) {
                        // iframe embedded documents will not be tracked for scroll right now
                    } else {
                        const children = elem.children;
                        for (let i = children.length - 1; i >= 0; i--) {
                            const e = this.rootFrame.cast(children[i], HTMLElement);
                            if (!e) {
                                continue;
                            }
                            stack.push({
                                elem: e,
                                path: `${path}-${i}`,
                                tag: `${tag}>${e.tagName.toLowerCase()}`,
                            });
                        }
                    }
                    processed++;
                }

                // Adapt batch size to target ~20ms of work per batch
                const elapsed = performance.now() - batchStartTime;
                if (elapsed > 0) {
                    batchSize = Math.max(1, Math.round(batchSize * (BATCH_TARGET_MS / elapsed)));
                } else {
                    batchSize = Math.min(batchSize * 2, BATCH_MAX_SIZE);
                }

                await sleep(0);

                if (this.controller.signal.aborted) {
                    return false;
                }
                if (this.changedWhileStarting) {
                    this.status.toast(
                        "red",
                        `the document structure changed while states are being tracked, state will not be tracked for the remaining elements.`,
                    );
                    break;
                }
            }
        } catch (e) {
            error(e);
        } finally {
            mutationWhileAddingElementObserver.disconnect();
        }

        log(`tracking state for ${elemCount} node(s)`);
        stop();

        return true;
    }

    public async apply(frame: Frame): Promise<void> {
        const body = frame.document()?.body;
        if (!body) {
            return;
        }
        let elemCount = 0;
        let windowScrollTop = 0;
        let windowScrollLeft = 0;
        for (const [path, entry] of this.state) {
            const { id, tag, scrollTop, scrollLeft } = entry;
            if (!path) {
                windowScrollTop = scrollTop;
                windowScrollLeft = scrollLeft;
                continue;
            }
            const elem = checkedElementAtPath(frame, body, id, path, tag);
            if (!elem) {
                continue;
            }
            const prevScrollBehavior = elem.style.scrollBehavior;
            if (getComputedStyle(elem).scrollBehavior === "smooth") {
                elem.style.scrollBehavior = "auto";
            }
            elem.scrollTop = scrollTop;
            elem.scrollLeft = scrollLeft;
            elem.style.scrollBehavior = prevScrollBehavior;
            elemCount++;
        }
        if (elemCount) {
            log(`applied state for ${elemCount} node(s)`);
        }
        if (windowScrollTop || windowScrollLeft) {
            const w = frame.window();
            if (w) {
                w.scrollTo(windowScrollLeft, windowScrollTop);
                log("applied window scroll");
            }
        }
    }

    /**
     * Wait for the document structure to stabilize (no childList mutations for
     * STABILIZE_DEBOUNCE_MS). Returns false if the document is still changing
     * after STABILIZE_TIMEOUT_MS, or if the signal is aborted.
     */
    private waitForStabilization(root: Node, debounceMs: number): Promise<boolean> {
        const waitStart = performance.now();
        let waitEnd = waitStart;
        return new Promise<boolean>((resolve) => {
            let debounceTimer: ReturnType<typeof setTimeout> | undefined = undefined;
            const done = (result: boolean) => {
                clearTimeout(debounceTimer);
                clearTimeout(absoluteTimer);
                observer.disconnect();
                this.lastStabilizationMs = waitEnd - waitStart;
                resolve(result);
            };
            const absoluteTimer = setTimeout(() => done(false), STABILIZE_TIMEOUT_MS);
            const resetDebounce = () => {
                waitEnd = performance.now();
                clearTimeout(debounceTimer);
                debounceTimer = setTimeout(() => done(true), debounceMs);
            };
            const observer = new MutationObserver(resetDebounce);
            observer.observe(root, { childList: true, subtree: true });
            this.controller.signal.addEventListener("abort", () => done(false), { once: true });
            // Kick off the debounce immediately in case the doc is already stable
            debounceTimer = setTimeout(() => done(true), debounceMs);
        });
    }
}

const isScrollable = (el: Element): boolean => {
    const { overflowX, overflowY } = getComputedStyle(el);
    const scrollableY =
        (overflowY === "auto" || overflowY === "scroll") && el.scrollHeight > el.clientHeight;
    if (scrollableY) {
        return true;
    }
    const scrollableX =
        (overflowX === "auto" || overflowX === "scroll") && el.scrollWidth > el.clientWidth;
    return scrollableX;
};

const checkedElementAtPath = (
    frame: Frame,
    body: HTMLElement,
    id: string,
    path: string,
    tag: string,
): HTMLElement | null => {
    if (!path) {
        return null;
    }
    const pathStack = path.split("-");
    const tagStack = tag.split(">");
    if (pathStack.length !== tagStack.length) {
        error("unexpected pathStack.length != tagStack.length");
        return null;
    }

    let cur: HTMLElement = body;
    for (let i = 0; i < pathStack.length; i++) {
        const index = +pathStack[i];
        const tag = tagStack[i];
        const child = frame.cast(cur.children[index], HTMLElement);
        if (!child || child.tagName.toLowerCase() !== tag) {
            return null;
        }
        if (id && child.id !== id) {
            return null;
        }
        cur = child;
    }
    return cur;
};
