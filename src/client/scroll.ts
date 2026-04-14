export type ScrollEntry = {
    tag: string;
    scrollTop: number;
    scrollLeft: number;
};

/** Map from tree path (e.g. "1-2-3") to scroll entry.
 * Path "1-2-3" means: 1st child of body > 2nd child > 3rd child. */
export type ScrollState = Map<string, ScrollEntry>;

const SCROLLABLE_TAGS = new Set([
    "article", "aside", "blockquote", "code", "details", "dialog",
    "div", "fieldset", "figure", "footer", "form", "h1", "h2", "h3",
    "h4", "h5", "h6", "header", "li", "main", "nav", "ol", "p", "pre",
    "section", "span", "summary", "table", "tbody", "td", "tfoot",
    "thead", "tr", "th", "ul",
]);

function isScrollable(el: Element): boolean {
    // only check if the element is white-listed to avoid checking a huge number of items
    if (!SCROLLABLE_TAGS.has(el.tagName.toLowerCase())) {
        return false;
    }
    const { overflowX, overflowY } = getComputedStyle(el);
    const scrollableY = (overflowY === "auto" || overflowY === "scroll")
        && el.scrollHeight > el.clientHeight;
    const scrollableX = (overflowX === "auto" || overflowX === "scroll")
        && el.scrollWidth > el.clientWidth;
    return scrollableY || scrollableX;
}

function treePath(el: Element, root: Element): string | null {
    const parts: number[] = [];
    let cur: Element = el;
    while (cur !== root) {
        const parent = cur.parentElement;
        if (!parent) return null;
        parts.unshift(Array.from(parent.children).indexOf(cur) + 1);
        cur = parent;
    }
    return parts.join("-");
}

function elementAtPath(root: Element, path: string): Element | null {
    let cur: Element = root;
    for (const part of path.split("-")) {
        const child = cur.children[+part - 1];
        if (!child) return null;
        cur = child;
    }
    return cur;
}

/** Add scroll listeners to all scrollable elements in doc.
 * Returns a function that snapshots the current scroll state. */
export function trackScrollState(doc: Document): () => ScrollState {
    const state: ScrollState = new Map();

    const walk = (el: Element, path: string) => {
        if (isScrollable(el)) {
            el.addEventListener("scroll", () => {
                const currentPath = treePath(el, doc.body);
                if (currentPath === null) return;
                state.set(currentPath, {
                    tag: el.tagName.toLowerCase(),
                    scrollTop: (el as HTMLElement).scrollTop,
                    scrollLeft: (el as HTMLElement).scrollLeft,
                });
            }, { passive: true });
        }
        const children = el.children;
        for (let i = 0; i < children.length; i++) {
            walk(children[i], `${path}-${i + 1}`);
        }
    };
    const bodyChildren = doc.body.children;
    for (let i = 0; i < bodyChildren.length; i++) {
        walk(bodyChildren[i], `${i + 1}`);
    }

    const observer = new MutationObserver((mutations) => {
        for (let i = 0; i < mutations.length; i++) {
            const added = mutations[i].addedNodes;
            for (let j = 0; j < added.length; j++) {
                const node = added[j];
                if (!(node instanceof Element)) continue;
                const path = treePath(node, doc.body);
                if (path === null) continue;
                walk(node, path);
            }
        }
    });
    observer.observe(doc.body, { childList: true, subtree: true });

    return () => new Map(state);
}

/** Apply a scroll state snapshot to doc.
 * Skips any entry where the element at that path has a different tag. */
export function applyScrollState(doc: Document, state: ScrollState): void {
    for (const [path, { tag, scrollTop, scrollLeft }] of state) {
        const el = elementAtPath(doc.body, path);
        if (!el || el.tagName.toLowerCase() !== tag) continue;
        (el as HTMLElement).scrollTop = scrollTop;
        (el as HTMLElement).scrollLeft = scrollLeft;
    }
}
