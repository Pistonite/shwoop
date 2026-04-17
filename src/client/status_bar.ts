export type StatusColor = "" | "green" | "red" | "yellow" | "cyan" | "orange";

const TOAST_DURATION_MS = 3000;

const hotReloadButton = document.querySelector(".status-bar-button") as HTMLButtonElement;
const stateRestoreButton = document.querySelector(
    ".status-bar-scroll-restore-button",
) as HTMLButtonElement;
let statusBarElement: HTMLDivElement | null = null;
let statusBarLabelWrapElement: HTMLSpanElement | null = null;
let statusBarLabelElement: HTMLSpanElement | null = null;

type Settings = {
    cold?: boolean;
    restoreDisabled?: boolean;
};
const STORAGE_KEY = "shwoop-settings";
const loadSettings = (): Settings => {
    try {
        return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    } catch {
        return {};
    }
};
const saveSettings = (patch: Partial<Settings>) => {
    try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...loadSettings(), ...patch }));
    } catch {
        /* ignore */
    }
};

let _settings: Settings = {};
let hotReload = !_settings.cold;
let stateRestoreEnabled = !_settings.restoreDisabled;

// The persistent status shown when no toast is active
let baseColor: StatusColor = "";
let baseMessage = "";
// Timer to revert back to base after a toast
let toastTimer: ReturnType<typeof setTimeout> | undefined = undefined;

export const initStatusBar = () => {
    _settings = loadSettings();
    statusBarElement = document.querySelector(".status-bar");
    if (statusBarElement) {
        statusBarElement.style.display = "";
    }
};

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
    if (!statusBarLabelWrapElement) {
        statusBarLabelWrapElement = document.querySelector(".status-bar-label-wrap");
    }
    if (!statusBarLabelElement) {
        statusBarLabelElement = document.querySelector(".status-bar-label");
    }
    const el = statusBarElement;
    const wrap = statusBarLabelWrapElement;
    const label = statusBarLabelElement;
    if (!el || !wrap || !label) {
        return;
    }

    // Snapshot current width, then measure the target width with new content
    const prevWidth = wrap.offsetWidth;
    wrap.style.transition = "none";
    wrap.style.width = "max-content";
    label.textContent = BIN_NAME + " " + x;
    const nextWidth = wrap.offsetWidth;

    // Jump back to prevWidth without animating, then apply class+width together
    // so both color and width transitions fire at the same time
    wrap.style.width = prevWidth + "px";
    void wrap.offsetWidth; // force reflow
    wrap.style.transition = "";
    el.className = `status-bar ${color}`;
    wrap.style.width = nextWidth + "px";
};

const updateHotReloadButton = () => {
    hotReloadButton.textContent = hotReload ? "🔥" : "🧊";
};
export const isHotReload = () => hotReload;
const updateStateRestoreButton = () => {
    stateRestoreButton.textContent = isStateRestoreEnabled() ? "💾" : "🚫";
};
export const isStateRestoreEnabled = () => hotReload && stateRestoreEnabled;

updateHotReloadButton();
hotReloadButton.addEventListener("click", () => {
    hotReload = !hotReload;
    saveSettings({ cold: !hotReload });
    updateHotReloadButton();
    updateStateRestoreButton();
    if (hotReload) {
        toast("orange", "reload mode: hot - content will be fetched and applied in the background");
    } else {
        toast("cyan", "reload mode: cold - the whole page will refresh immediately");
    }
});

updateStateRestoreButton();
stateRestoreButton.addEventListener("click", () => {
    if (!hotReload) {
        toast("cyan", "state restore is only possible in hot reload mode");
        return;
    }
    stateRestoreEnabled = !stateRestoreEnabled;
    saveSettings({ restoreDisabled: !stateRestoreEnabled });
    updateStateRestoreButton();
    if (stateRestoreEnabled) {
        toast("green", "state restore: on - some states will be restored on reload");
    } else {
        toast("red", "state restore: off - states will not be restored on reload");
    }
});
