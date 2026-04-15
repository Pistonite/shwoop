/**
 * Handle the case when user clicks on a link inside a wrapped iframe.
 *
 * In that case, the frame will navigate to that page, which will load
 * another wrapped page. So we want to immediately check if we are inside
 * a wrapped iframe. If so, we tell the top (hosting) page to navigate there
 * if it's not already there.
 */
export const handleLocationSwitchInIFrame = (): boolean => {
    if (isInWrappedIFrame()) {
        return true;
    }

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (window as any)["__shwoop_top_page"] = true;
    return false;
};

const isInWrappedIFrame = (): boolean => {
    try {
        if (window.self === window.top) {
            // we are top frame, not wrapped
            return false;
        }
        const topWindow = window.top;
        if (!topWindow) {
            return false;
        }
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        if (!(topWindow as any)["__shwoop_top_page"]) {
            return false;
        }
        const l = globalThis.location;
        topWindow.location.href = l.pathname + l.search + l.hash;
        return true;
    } catch {
        // access error, must mean we are not wrapped
        return false;
    }
};

export const getUrlWithRawQuery = (l: Location): string => {
    let search = l.search;
    if (search.startsWith("?")) {
        search += "&";
    } else {
        search = "?";
    }
    search += "x-shwoop-is-raw=1";
    return `${l.pathname}${search}${l.hash}`;
};
