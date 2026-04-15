const DELAYED_CALL_MS = 500;

/** Call fn after DELAYED_CALL_MS. Returns a cancel function; if called before the delay fires, fn is not invoked. */
export const delayed = (fn: () => void): () => void => {
    const timer = setTimeout(fn, DELAYED_CALL_MS);
    return () => clearTimeout(timer);
};

export const sleep = (ms: number): Promise<void> => {
    return new Promise((resolve) => {
        setTimeout(resolve, ms)
    });
}

export const log = (...x: unknown[]) => {
    console.log(BIN_NAME, ...x);
}

export const error = (...x: unknown[]) => {
    console.log(BIN_NAME, ...x);
}

export const HOST = 
import.meta.env.DEV ? `${globalThis.location.hostname}:8241` : globalThis.location.host;
