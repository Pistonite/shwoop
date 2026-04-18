(function() {
    // script injected into served HTML pages to inject the client
    function getTopWindowIfInShwoopIFrame() {
        if (window.self === window.top) {
            return null;
        }
        const topWindow = window.top;
        if (!topWindow||!topWindow["__shwoop_top_page"]) {
            return null;
        }
        return topWindow;
    }
    try {
        const topWindow = getTopWindowIfInShwoopIFrame();
        if (topWindow) {
            // we are already in an iframe hosted by the client, stop, don't inject the client again
            // however we need to fix the history state if links were clicked from the frame
            const correctPath = location.pathname+location.search+location.hash;
            const topLocation = topWindow.location;
            const topPath = topLocation.pathname+topLocation.search+topLocation.hash;
            if (topPath !== correctPath) {
                topWindow.history.pushState({
                    pathname: location.pathname,
                    search: location.search,
                    hash: location.hash,
                }, "", correctPath);
            }
            return;
        }
    } catch { }
    // inject the client
    const script = document.createElement("script");
    script.type="module";
    script.src="/shwoop-bf52606e-9f65-4f5e-8a2f-016ad7cfa92a-xxx-DIs426w5.js";
    document.head.appendChild(script);
    console.log("[shwoop] bootstrap: injected client");
})()