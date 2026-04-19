import { injectStyle } from "./style";
import { displayMs } from "./util";

export const StatusColor = {
    green: "#8f8",
    yellow: "#ff8",
    red: "#f88",
} as const;
export type StatusColor = "" | keyof typeof StatusColor;
const STORAGE_KEY = BIN_NAME + "--settings";
const CLASSNAME = BIN_NAME + "--status-bar";

export const PerfNameMap = [
    ["build", "build"],
    ["page-load", "page load"],
    ["stabilize", "page stabilize"],
    ["track", "state traversal"]
] as const;
export type PerfName = (typeof PerfNameMap)[number][0];

export class StatusBar {
    private settings: Settings;
    private div: HTMLDivElement;
    private labelContainer: HTMLSpanElement;
    private label: HTMLSpanElement;
    private buttonHotReload: HTMLButtonElement;
    private buttonStateTracking: HTMLButtonElement;
    private baseMessage: string = "";
    private baseColor: StatusColor = "";
    private toastTimer: ReturnType<typeof setTimeout> | undefined;

    private perfData: { [K in PerfName]: number };

    constructor() {
        this.perfData = {
            ["build"]: -1,
            ["page-load"]: -1,
            ["stabilize"]: -1,
            ["track"]: -1,
        };
        this.settings = {
            enableHotReload: true,
            enableStateTracking: true,
        };
        try {
            const storedSettings = JSON.parse(localStorage.getItem(STORAGE_KEY) || "{}");
            this.settings = {
                ...this.settings,
                ...storedSettings,
            };
        } catch {
            // ignore
        }

        this.div = document.createElement("div");
        this.div.className = CLASSNAME;
        this.labelContainer = document.createElement("span");
        this.labelContainer.className = CLASSNAME + "-labelcontainer";
        this.label = document.createElement("span");
        this.buttonHotReload = document.createElement("button");
        this.buttonHotReload.title = "Toggle hot/cold reload";
        this.buttonHotReload.addEventListener("click", () => {
            this.settings.enableHotReload = !this.settings.enableHotReload;
            this.saveSettings();
            this.updateButtons();
            if (this.settings.enableHotReload) {
                this.toast(
                    "green",
                    "reload mode: hot - content will be fetched and applied in the background",
                );
            } else {
                this.toast("yellow", "reload mode: cold - the whole page will refresh immediately");
            }
        });
        this.buttonStateTracking = document.createElement("button");
        this.buttonStateTracking.title = "Toggle state tracking";
        this.buttonStateTracking.addEventListener("click", () => {
            this.settings.enableStateTracking = !this.settings.enableStateTracking;
            this.saveSettings();
            this.updateButtons();
            if (this.settings.enableStateTracking) {
                this.toast("green", "state tracking: on - some states will be restored on reload");
            } else {
                this.toast("red", "state tracking: off - states will not be restored on reload");
            }
        });
        const buttonPerf = document.createElement("button");
        buttonPerf.title = "Show performance";
        buttonPerf.textContent = "🕙";
        buttonPerf.addEventListener("click", () => {
            const messages: string[] = [];
            for (const [key, name] of PerfNameMap) {
                const value = this.perfData[key];
                if (value >= 0) {
                    messages.push(`${name}: ${displayMs(value)}`);
                }
            }

            if (messages.length) {
                this.toast("yellow", messages.join(" | "));
            } else {
                this.toast("yellow", "no data yet");
            }
        });

        const divButtons = document.createElement("div");
        divButtons.className = CLASSNAME + "-buttons";
        divButtons.append(this.buttonHotReload, this.buttonStateTracking, buttonPerf);

        this.labelContainer.append(this.label);
        this.div.append(this.labelContainer, divButtons);

        // note here right: 24px
        // this accounts for the scrollbar in the iframe.
        // however this leads to the weird quirk that in the first-render,
        // there's no iframe (the page is directly in the top level document),
        // so there is actually the window scroll bar, so the status bar
        // appears shifted to the left.
        // with some size observer and passing data between frames, this can
        // be fixed, but it's not worth the effort
        // this is now a "feature" to tell if the document has been reloaded, lmao

        let styles = `
            .${CLASSNAME} {
                position: fixed; bottom: 0; right: 24px; height: 24px;
                z-index: 9999;
                font-family: ui-monospace, Menlo, Consolas, monospace;
                font-size: 12px;
                box-sizing: border-box;
                border: 1px solid #888;
                border-radius: 4px;
                border-top-left-radius: 4px;
                border-bottom-left-radius: 4px;
                background: #222;
                color: #eee;
                white-space: nowrap;
                display: flex;
                align-items: center;
                transition: color 0.2s ease;
            }
            .${CLASSNAME}-labelcontainer {
                overflow: hidden;
                transition: width 0.2s ease;
            }
            .${CLASSNAME}-buttons {
                display: flex;
                align-items: center;
                gap: 2px;
                padding-left: 4px;
            }
            .${CLASSNAME} button {
                all: unset;
                display: inline-flex;
                align-items: center;
                justify-content: center;
                width: 20px; height: 20px;
                border-radius: 3px;
                font-size: 13px;
                line-height: 1;
                cursor: pointer;
                transition: background 0.15s ease;
            }
            .${CLASSNAME} button:hover  { background: #3a3a3a; }
            .${CLASSNAME} button:active { background: #4a4a4a; }
        `;
        for (const color in StatusColor) {
            styles += `.${CLASSNAME}-label.${color} { color: ${StatusColor[color as keyof typeof StatusColor]} }`;
        }
        injectStyle(
            CLASSNAME + "--styles",
            styles
                .split("\n")
                .map((x) => x.trim())
                .join(" "),
        );

        this.updateButtons();

        document.body.append(this.div);
    }

    public element(): HTMLDivElement {
        return this.div;
    }

    private saveSettings() {
        try {
            localStorage.setItem(STORAGE_KEY, JSON.stringify(this.settings));
        } catch {
            // ignore
        }
    }

    public isHotReloadEnabled(): boolean {
        return !!this.settings.enableHotReload;
    }

    public isStateTrackingEnabled(): boolean {
        return this.isHotReloadEnabled() && !!this.settings.enableStateTracking;
    }

    /** Set color and message of the status bar and display immediately */
    public update(color: StatusColor, message: string) {
        this.updateBaseOnly(color, message);
        clearTimeout(this.toastTimer);
        this.toastTimer = undefined;
        this.updateLabel(color, message);
    }

    public toast(color: StatusColor, message: string) {
        clearTimeout(this.toastTimer);
        this.toastTimer = undefined;
        this.updateLabel(color, message);
        // ~80ms per character, clamped to [2s, 10s]
        const duration = Math.min(10000, Math.max(3000, message.length * 80));
        this.toastTimer = setTimeout(() => {
            this.toastTimer = undefined;
            this.updateLabel(this.baseColor, this.baseMessage);
        }, duration);
    }

    /** Set color and message of the status bar, but don't override current toast*/
    public updateBaseOnly(color: StatusColor, message: string) {
        this.baseColor = color;
        this.baseMessage = message;
    }

    private updateButtons() {
        this.buttonHotReload.textContent = this.isHotReloadEnabled() ? "🔥" : "🧊";
        this.buttonStateTracking.textContent = this.isStateTrackingEnabled() ? "💾" : "🚫";
    }

    private updateLabel(color: StatusColor, message: string) {
        // Snapshot current width, then measure the target width with new content
        const prevWidth = this.labelContainer.offsetWidth;
        this.labelContainer.style.transition = "none";
        this.labelContainer.style.width = "max-content";
        this.label.textContent = "[" + BIN_NAME + "] " + message;
        const nextWidth = this.labelContainer.offsetWidth;

        // Jump back to prevWidth without animating, then apply class+width together
        // so both color and width transitions fire at the same time
        this.labelContainer.style.width = prevWidth + "px";
        void this.labelContainer.offsetWidth; // force reflow
        this.labelContainer.style.transition = "";
        this.label.className = `${BIN_NAME}--status-bar-label ${color}`;
        this.labelContainer.style.width = nextWidth + "px";
    }

    public updatePerfNumber(name: PerfName, num: number) {
        this.perfData[name] = num;
    }

    public startPerfMeasure(name: PerfName): () => void {
        const start = performance.now();
        return () => {
            this.perfData[name] = performance.now() - start;
        };
    }
}

type Settings = {
    enableHotReload?: boolean;
    enableStateTracking?: boolean;
};
