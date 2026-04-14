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
