(function () {
    // script injected into served HTML pages to inject the client
    function getTopWindowIfInShwoopIFrame() {
        if (window.self === window.top) {
            return null;
        }
        const parent = window.parent;
        if (!parent || !parent["__shwoop_top_page"]) {
            return null;
        }
        return parent;
    }
    function navigateParent(url, replace) {
        if (!window.parent) {
            return false;
        }
        if (window.parent.location.href === url) {
            return false;
        }
        if (replace) {
            window.parent.location.replace(url);
        } else {
            window.parent.location.href = url;
        }
        return true;
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
                navigateParent(correctPath, replace);
                return;
            }
            // patch all navigation to route through parent
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
            history.pushState = function (state, unused, url) {
                if (url) {
                    if (navigateParent(String(url))) {
                        return;
                    }
                }
                _pushState(state, unused, url);
            };
            history.replaceState = function (state, unused, url) {
                if (url) {
                    if (navigateParent(String(url), true)) {
                        return;
                    }
                }
                _replaceState(state, unused, url);
            };
            // location.assign / replace / href setter
            const _assign = location.assign.bind(location);
            const _replace = location.replace.bind(location);
            location.assign = function (url) {
                if (!navigateParent(url)) _assign(url);
            };
            location.replace = function (url) {
                if (!navigateParent(url, true)) _replace(url);
            };
            try {
                const proto = Object.getPrototypeOf(location);
                const desc = Object.getOwnPropertyDescriptor(proto, "href");
                Object.defineProperty(location, "href", {
                    get: desc.get.bind(location),
                    set(url) {
                        if (!navigateParent(url)) desc.set.call(location, url);
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
