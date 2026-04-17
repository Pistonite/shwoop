import { defineConfig } from "mono-dev/vite";
import monodev from "mono-dev/vite-config";


// need to be kept sync with src/server/http.rs
// see comments in that file
const JS_CHUNK_NAME = "bf52606e-9f65-4f5e-8a2f-016ad7cfa92a_please_dont_collide";

const monodevConfig = monodev({});
export default defineConfig(
    monodevConfig({
        define: {
            BIN_NAME: "'[shwoop]'",
        },
        plugins: [],
        build: {
            // tried inline sourcemap but that doesn't really work for some reason
            // (could be because of the inline plugin?)
            rollupOptions: {
                output: {
                    entryFileNames: JS_CHUNK_NAME + ".js"
                }
            }
        },
    }),
);
