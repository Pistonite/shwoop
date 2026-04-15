import { error, log, sleep } from "./util.ts";

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

export class ScrollTracker {
    private controller: AbortController;
    private state: Map<string, ScrollEntry>;
    private changedWhileStarting: boolean;

    constructor(private rootFrame: HTMLIFrameElement) {
        this.controller = new AbortController();
        this.state = new Map();
        this.changedWhileStarting = false;
    }

    public stop() {
        this.controller.abort();
    }

    public async start(): Promise<boolean> {
        const rootBody = safeGetFrameDocuemnt(this.rootFrame)?.body;
        if (!rootBody) {
            error("failed to get top-frame body, scroll will not be tracked");
            return false;
        }
        if (!await waitForStabilization(rootBody, this.controller.signal)) {
            error(`the document structure did not stablize after ${STABILIZE_TIMEOUT_MS}, scroll will not be tracked`);
            return false;
        }
        const mutationWhileAddingElementObserver = new MutationObserver(() => {
            this.changedWhileStarting = true;
            mutationWhileAddingElementObserver.disconnect();
        });
        mutationWhileAddingElementObserver.observe(rootBody, { childList: true, subtree: true });
        this.controller.signal.addEventListener("abort", () => mutationWhileAddingElementObserver.disconnect(), { once: true });

        const startTime = performance.now();
        let elemCount = 0;
        rootBody.addEventListener("scroll", () => {
            const sTop = rootBody.scrollTop;
            const sLeft = rootBody.scrollLeft;
                            if (sTop && sLeft) {
                                this.state.set("", {
                    id: "",
                                    tag: "",
                                    scrollTop: sTop,
                                    scrollLeft: sLeft,
                                });
                            } else {
                                this.state.delete("");
                            }
        });

        try {
            const stack: StackItem[] = [];
            const bodyChildren = rootBody.children;
            for (let i = bodyChildren.length - 1; i >= 0; i--) {
                const e = bodyChildren[i];
                if (e instanceof HTMLElement) {
                    stack.push({ elem: e, path: `${i}`, tag: e.tagName.toLowerCase() });
                }
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
                        elem.addEventListener("scroll", () => {
                            const sTop = elem.scrollTop;
                            const sLeft = elem.scrollLeft;
                            if (sTop && sLeft) {
                                this.state.set(path, {
                                    id: elem.id,
                                    tag,
                                    scrollTop: sTop,
                                    scrollLeft: sLeft,
                                });
                            } else {
                                this.state.delete(path);
                            }

                        }, { passive: true, signal: this.controller.signal });
                    }

                    if (elem instanceof HTMLIFrameElement) {
                        // iframe embedded documents will not be tracked for scroll right now
                    } else {
                        const children = elem.children;
                        for (let i = children.length - 1; i >= 0; i--) {
                            const e = children[i];
                            if (e instanceof HTMLElement) {
                                stack.push({ elem: e, path: `${path}-${i}`, tag: `${tag}>${e.tagName.toLowerCase()}` });
                            }
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
                    error(`the document structure changed while scroll states are being tracked, scroll will not be tracked for the remaining elements.`);
                    break;
                }
            }
        } finally {
            mutationWhileAddingElementObserver.disconnect();
        }

        const elapsed = Math.floor(performance.now() - startTime);
        log(`took ${elapsed}ms to start tracking scrolling for ${elemCount} nodes`);

        return true;
    }

    public apply(frame: HTMLIFrameElement) {
        const body = safeGetFrameDocuemnt(frame)?.body;
        if (!body) {
            return;
        }
        let elemCount = 0;
        for (const [path, { id, tag, scrollTop, scrollLeft }] of this.state) {
            const elem = checkedElementAtPath(body, id, path, tag);
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
        log(`apply scroll state for ${elemCount} nodes`);
    }
}


/**
 * Wait for the document structure to stabilize (no childList mutations for
 * STABILIZE_DEBOUNCE_MS). Returns false if the document is still changing
 * after STABILIZE_TIMEOUT_MS, or if the signal is aborted.
 */
const waitForStabilization = (root: Node, signal: AbortSignal): Promise<boolean> => {
    return new Promise<boolean>((resolve) => {
        let debounceTimer: ReturnType<typeof setTimeout> | undefined = undefined;
        const done = (result: boolean) => {
            clearTimeout(debounceTimer);
            clearTimeout(absoluteTimer);
            observer.disconnect();
            resolve(result);
        };
        const absoluteTimer = setTimeout(() => done(false), STABILIZE_TIMEOUT_MS);
        const resetDebounce = () => {
            clearTimeout(debounceTimer);
            debounceTimer = setTimeout(() => done(true), STABILIZE_DEBOUNCE_MS);
        };
        const observer = new MutationObserver(resetDebounce);
        observer.observe(root, { childList: true, subtree: true });
        signal.addEventListener("abort", () => done(false), { once: true });
        // Kick off the debounce immediately in case the doc is already stable
        resetDebounce();
    });
};

const safeGetFrameDocuemnt = (frame: HTMLIFrameElement): Document | null => {
    try {
        return frame.contentDocument;
    } catch {
        return null;
    }
}

const isScrollable = (el: Element): boolean => {
    const { overflowX, overflowY } = getComputedStyle(el);
    const scrollableY = (overflowY === "auto" || overflowY === "scroll")
        && el.scrollHeight > el.clientHeight;
    if (scrollableY) {
        return true;
    }
    const scrollableX = (overflowX === "auto" || overflowX === "scroll")
        && el.scrollWidth > el.clientWidth;
    return scrollableX;
}


const checkedElementAtPath = (body: HTMLElement, id: string, path: string, tag: string): HTMLElement | null => {
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
        const child = cur.children[index];
        if (!child || !(child instanceof HTMLElement) || child.tagName.toLowerCase() !== tag) {
            return null;
        }
        if (id && child.id !== id) {
            return null;
        }
        cur = child;
    }
    return cur;
}
