
export type StatusColor = "" | "green" | "red" | "yellow" | "cyan" | "orange";

const TOAST_DURATION_MS = 3000;

const hotReloadButton = document.querySelector(".status-bar-button") as HTMLButtonElement;
const scrollRestoreButton = document.querySelector(".status-bar-scroll-restore-button") as HTMLButtonElement;
let statusBarElement: HTMLDivElement | null = null;
let statusBarLabelElement: HTMLSpanElement | null = null;

type Settings = { cold?: boolean; scrollRestoreDisabled?: boolean };
const STORAGE_KEY = "shwoop-settings";
const loadSettings = (): Settings => {
    try { return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}"); } catch { return {}; }
};
const saveSettings = (patch: Partial<Settings>) => {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...loadSettings(), ...patch })); } catch { /* ignore */ }
};

let _settings: Settings = {};
let hotReload = !_settings.cold;
let scrollRestoreEnabled = !_settings.scrollRestoreDisabled;

// The persistent status shown when no toast is active
let baseColor: StatusColor = "";
let baseMessage = "";
// Timer to revert back to base after a toast
let toastTimer: ReturnType<typeof setTimeout> | undefined = undefined;

export const initStatusBar = () => {
    _settings = loadSettings();
    statusBarElement = document.querySelector(".status-bar");
    if (statusBarElement) {
        statusBarElement.style.display="";
    }

}

export const isHotReload = () => hotReload;

export const toast = (color: StatusColor, x: string) => {
    clearTimeout(toastTimer);
    applyStatus(color, x);
    toastTimer = setTimeout(() => {
        toastTimer = undefined;
        applyStatus(baseColor, baseMessage);
    }, TOAST_DURATION_MS);
};

export const status = (color: StatusColor, x: string, apply = true) => {
    clearTimeout(toastTimer);
    toastTimer = undefined;
    baseColor = color;
    baseMessage = x;
    if (apply) {
        applyStatus(color, x);
    }
};

const applyStatus = (color: StatusColor, x: string) => {
    if (!statusBarElement) {
        statusBarElement = document.querySelector(".status-bar");
    }
    if (!statusBarLabelElement) {
        statusBarLabelElement = document.querySelector(".status-bar-label");
    }
    const el = statusBarElement;
    const label = statusBarLabelElement;
    if (!el || !label) {
        return;
    }

    // Snapshot current width, then measure the target width with new content
    const prevWidth = el.offsetWidth;
    el.style.transition = "none";
    el.style.width = "max-content";
    label.textContent = BIN_NAME + " " + x;
    const nextWidth = el.offsetWidth;

    // Jump back to prevWidth without animating, then apply class+width together
    // so both color and width transitions fire at the same time
    el.style.width = prevWidth + "px";
    void el.offsetWidth; // force reflow
    el.style.transition = "";
    el.className = `status-bar ${color}`;
    el.style.width = nextWidth + "px";
};

const updateHotReloadButton = () => {
    hotReloadButton.textContent = hotReload ? "🔥" : "🧊";
};
updateHotReloadButton();
hotReloadButton.addEventListener("click", () => {
    hotReload = !hotReload;
    saveSettings({ cold: !hotReload });
    updateHotReloadButton();
    if (hotReload) {
        toast("orange", "reload mode: hot - content will be fetched and applied in the background");
    } else {
        toast("cyan", "reload mode: cold - the whole page will refresh immediately");
    }
});

const updateScrollRestoreButton = () => {
    scrollRestoreButton.textContent = scrollRestoreEnabled ? "📜" : "🚫";
};
updateScrollRestoreButton();
scrollRestoreButton.addEventListener("click", () => {
    scrollRestoreEnabled = !scrollRestoreEnabled;
    saveSettings({ scrollRestoreDisabled: !scrollRestoreEnabled });
    updateScrollRestoreButton();
    if (scrollRestoreEnabled) {
        toast("green", "scroll restore: on - scroll positions will be restored on reload");
    } else {
        toast("red", "scroll restore: off - scroll positions will not be restored on reload");
    }
});

export const isScrollRestoreEnabled = () => scrollRestoreEnabled;
