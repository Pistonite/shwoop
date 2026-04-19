(function () {
    // script injected into served HTML pages to inject the client
    function getTopWindowIfInShwoopIFrame() {
        if (window.self === window.top) {
            return null;
        }
        const topWindow = window.parent;
        if (!topWindow || !topWindow["__shwoop_top_page"]) {
            return null;
        }
        return topWindow;
    }
    try {
        const topWindow = getTopWindowIfInShwoopIFrame();
        if (topWindow) {
            // we are already in an iframe hosted by the client, stop, don't inject the client again
            // however we need to fix the history state if links were clicked from the frame
            const correctPath = location.pathname + location.search + location.hash;
            const topLocation = topWindow.location;
            const topPath = topLocation.pathname + topLocation.search + topLocation.hash;
            if (topPath !== correctPath) {
                topWindow.history.pushState(
                    {
                        pathname: location.pathname,
                        search: location.search,
                        hash: location.hash,
                    },
                    "",
                    correctPath,
                );
            }
            // Patch all navigation to route through parent
            const parentWindow = window.parent;
            function navigateParent(url, replace) {
                try {
                    const parsed = new URL(url, location.href);
                    const path = parsed.pathname + parsed.search + parsed.hash;
                    if (parsed.origin === location.origin) {
                        if (replace) {
                            parentWindow.history.replaceState({}, "", path);
                        } else {
                            parentWindow.history.pushState({}, "", path);
                        }
                        return;
                    }
                } catch {}
                if (replace) {
                    parentWindow.location.replace(url);
                } else {
                    parentWindow.location.href = url;
                }
            }
            // anchor clicks
            document.addEventListener(
                "click",
                function (e) {
                    const a = e.target.closest("a");
                    if (!a || !a.href || e.defaultPrevented) return;
                    if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
                    if (a.target && a.target !== "_self") return;
                    e.preventDefault();
                    navigateParent(a.href);
                },
                true,
            );
            // history API (SPA routing)
            const _pushState = history.pushState.bind(history);
            const _replaceState = history.replaceState.bind(history);
            history.pushState = function (state, title, url) {
                if (url != null) {
                    navigateParent(String(url));
                    return;
                }
                _pushState(state, title, url);
            };
            history.replaceState = function (state, title, url) {
                if (url != null) {
                    navigateParent(String(url), true);
                    return;
                }
                _replaceState(state, title, url);
            };
            // location.assign / replace / href setter
            location.assign = function (url) {
                navigateParent(url);
            };
            location.replace = function (url) {
                navigateParent(url, true);
            };
            try {
                const proto = Object.getPrototypeOf(location);
                const desc = Object.getOwnPropertyDescriptor(proto, "href");
                Object.defineProperty(proto, "href", {
                    get: desc.get,
                    set(url) {
                        navigateParent(url);
                    },
                    configurable: true,
                });
            } catch {}
            return;
        }
    } catch {}
    // inject the client
    window["__shwoop_top_page"] = true;
    const script = document.createElement("script");
    script.type = "module";
    script.src = PLACEHOLDER_VITE_SCRIPT_SOURCE;
    script.dataset.isshwoop = "true";
    document.head.appendChild(script);

    const style = document.createElement("style");
    style.innerText =
        ":root { color-scheme: light dark } body {padding:0;margin:0} .shwoop--content-frame { position:fixed;top:0;left:0;right:0;bottom:0;border:none;width:100vw;height:100vh; }";
    style.dataset.isshwoop = "true";
    document.head.appendChild(style);
    console.log("[shwoop] bootstrap: injected client");
})();
