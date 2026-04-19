// from @pistonite/celera
export function injectStyle(id: string, style: string) {
    const styleTags = document.querySelectorAll(`style[data-inject="${id}"]`);
    if (styleTags.length !== 1) {
        const styleTag = document.createElement("style");
        styleTag.dataset.inject = id;
        styleTag.dataset.isshwoop = "true";
        styleTag.innerText = style;
        document.head.appendChild(styleTag);
        setTimeout(() => {
            styleTags.forEach((tag) => tag.remove());
        }, 0);
    } else {
        const e = styleTags[0] as HTMLStyleElement;
        if (e.innerText !== style) {
            e.innerText = style;
        }
    }
}
