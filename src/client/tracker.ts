import { toast } from "./status_bar.ts";
import { type Class, error, log, sleep } from "./util.ts";

type ScrollEntry = {
    id: string;
    /** tag stack from body (e.g. div>div>p) */
    tag: string;
    scrollTop: number;
    scrollLeft: number;
};

type StackItem = {
    /** The element to process. */
    elem: HTMLElement;
    /** Full path to this element **at the time of stabilization** */
    path: string;
    /** tag stack from body, separated by > */
    tag: string;
};

const BATCH_TARGET_MS = 50;
const BATCH_MAX_SIZE = 10000;
const STABILIZE_DEBOUNCE_MS = 1000;
const STABILIZE_TIMEOUT_MS = 2000;

/** Tracks state of the document that can be re-applied when reloading */
export class StateTracker {
    private controller: AbortController;
    private state: Map<string, ScrollEntry>;
    private changedWhileStarting: boolean;

    constructor(
        private rootFrame: HTMLIFrameElement,
        private lastStabilizationMs: number | undefined
    ) {
        this.controller = new AbortController();
        this.state = new Map();
        this.changedWhileStarting = false;
    }

    public stop() {
        this.controller.abort();
    }

    public getLastStabilizationTimeMs(): number | undefined {
        return this.lastStabilizationMs;
    }

    public async start(): Promise<boolean> {
        const rootBody = safeGetFrameDocuemnt(this.rootFrame)?.body;
        if (!rootBody) {
            toast("red", "failed to get top-frame body, state will not be tracked");
            return false;
        }
        const stabilizationDebounceMs = this.lastStabilizationMs ? this.lastStabilizationMs * 2 : STABILIZE_DEBOUNCE_MS;
        if (!(await this.waitForStabilization(rootBody, stabilizationDebounceMs))) {
            toast("red",
                `the document structure did not stablize after ${stabilizationDebounceMs}, state will not be tracked`,
            );
            return false;
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

        const startTime = performance.now();
        let elemCount = 0;
        rootBody.addEventListener("scroll", () => {
            const sTop = rootBody.scrollTop;
            const sLeft = rootBody.scrollLeft;
            if (sTop || sLeft) {
                this.state.set("", {
                    id: "",
                    tag: "",
                    scrollTop: sTop,
                    scrollLeft: sLeft,
                });
            } else {
                this.state.delete("");
            }
        }, {passive:true, signal: this.controller.signal});

        try {
            const stack: StackItem[] = [];
            const bodyChildren = rootBody.children;
            for (let i = bodyChildren.length - 1; i >= 0; i--) {
                const e = this.iframeElemCast(bodyChildren[i], HTMLElement);
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

                    if (this.iframeElemCast(elem, HTMLIFrameElement)) {
                        // iframe embedded documents will not be tracked for scroll right now
                    } else {
                        const children = elem.children;
                        for (let i = children.length - 1; i >= 0; i--) {
                            const e = this.iframeElemCast(children[i], HTMLElement);
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
                    toast("red",
                        `the document structure changed while states are being tracked, state will not be tracked for the remaining elements.`,
                    );
                    break;
                }
            }
        } catch(e) {
            error(e);
        } finally {
            mutationWhileAddingElementObserver.disconnect();
        }

        const elapsed = Math.floor(performance.now() - startTime);
        log(`took ${elapsed}ms to start tracking state for ${elemCount} nodes`);

        return true;
    }

    public apply(frame: HTMLIFrameElement) {
        const body = safeGetFrameDocuemnt(frame)?.body;
        if (!body) {
            return;
        }
        let elemCount = 0;
        for (const [path, entry
    ] of this.state) {
            console.log(path, entry);
        const { id, tag, scrollTop, scrollLeft } = entry;
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
        log(`applied state for ${elemCount} nodes`);
    }

    /**
     * Cast e to a sub HTMLElement type. This is needed because each frame has a different HTMLElement class
     * so we need to use the frame's class to do the instanceof check
     */
    private iframeElemCast<T extends Element>(e: Element | null, clazz: Class<T>): T | null {
        return iframeElemCast(this.rootFrame, e, clazz);
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
};

}


const safeGetFrameDocuemnt = (frame: HTMLIFrameElement): Document | null => {
    try {
        return frame.contentDocument;
    } catch {
        return null;
    }
};

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
    frame: HTMLIFrameElement,
    body: HTMLElement,
    id: string,
    path: string,
    tag: string,
): HTMLElement | null => {
    if (!path) {
        return body;
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
        const child = iframeElemCast(frame, cur.children[index], HTMLElement);
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
    /**
     * Cast e to a sub HTMLElement type. This is needed because each frame has a different HTMLElement class
     * so we need to use the frame's class to do the instanceof check
     */
    const iframeElemCast = <T extends Element>(frame: HTMLIFrameElement, e: Element | null, clazz: Class<T>): T | null => {
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
            };
        } catch {
            // fallthrough
        }
        return null;
    }
