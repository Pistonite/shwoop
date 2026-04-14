import { defineConfig } from "mono-dev/vite";
import monodev from "mono-dev/vite-config";
import inline from "@zhoumutou/vite-plugin-inline";

const monodevConfig = monodev({});
export default defineConfig(monodevConfig({
    define: {
        "BIN_NAME": "'[shwoop]'",
    },
    plugins: [inline()],
    build: {
        sourcemap: "inline"
    },
}));
