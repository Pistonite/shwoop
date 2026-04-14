
export type StatusColor = "" | "green" | "red" | "yellow";

let statusBarElement: HTMLDivElement | null = null;

export const status = (color: StatusColor, x: string) => {
    if (!statusBarElement) {
        statusBarElement = document.querySelector(".status-bar");
    }
    const el = statusBarElement;
    if (!el) {
        return;
    }

    // Snapshot current width, then measure the target width with new content
    const prevWidth = el.offsetWidth;
    el.style.transition = "none";
    el.style.width = "max-content";
    el.textContent = BIN_NAME + " " + x;
    const nextWidth = el.offsetWidth;

    // Jump back to prevWidth without animating, then apply class+width together
    // so both color and width transitions fire at the same time
    el.style.width = prevWidth + "px";
    el.offsetWidth; // force reflow
    el.style.transition = "";
    el.className = `status-bar ${color}`;
    el.style.width = nextWidth + "px";
}
