import fs from "node:fs/promises";
import path from "node:path";

import { type Plugin } from "mono-dev/vite";
import { configure } from "mono-dev/app-build-config";

// kind of a hack - using vite's app mode since mono-dev lib-build mode
// will also produce type definition which we don't need.
// but the HTML is just a stub since we don't serve that page directly.
// instead we inject a bootstrap script into the pages served to start the reloading client
const writeIndexHtml = async () => {
    const SRC = "/src/main.ts";
    await fs.writeFile(
        path.join(import.meta.dirname, "index.html"),
        `<!doctype html><html><head></head><body><script type="module" src="${SRC}"></script></body></html>`,
    );
};

// note on chunk name:
// Ideally, the JS should be inlined into the HTML, so the server only
// needs to route HTML pages through the wrapper page for hot-reload to work.
// The wrapper HTML would be self-contained, and the server doesn't need
// additional routes to handle hot-reload related files (since they
// could collide with the actual files to serve).
// However, there is just one issue - when inlining the script directly
// into a script tag and inlining the sourcemap as a data URL into the script,
// the source label stops working. All lines got mapped to the last line of main.ts
// for some reason.
// I don't have time to fix it right now, so here's a workaround to make
// a path that's unlikely to collide, and have the server serve the JS
// and the source map
const JS_CHUNK_NAME = "shwoop-bf52606e-9f65-4f5e-8a2f-016ad7cfa92a";

const postProcess = (): Plugin => {
    return {
        name: "shwoop-post-process",
        apply: "build",

        closeBundle: async () => {
            const srcDir = path.resolve(import.meta.dirname, "src");
            const distDir = path.resolve(import.meta.dirname, "dist");
            let files: string[] = [];
            try {
                files = (await fs.readdir(distDir)).filter((f) => f.endsWith(".js.map"));
            } catch {
                console.error("failed to read dist, not post processing");
                return;
            }
            if (files.length === 0) {
                throw new Error("No .js.map file found in dist");
            }
            if (files.length > 1) {
                throw new Error(`Expected 1 .js.map file in dist, found ${files.length}`);
            }
            const name = files[0];
            const sourceName = name.substring(0, name.length - 4);

            let bootstrapJs = await fs.readFile(path.join(srcDir, "bootstrap.js"), "utf-8");
            bootstrapJs = bootstrapJs
                .replace("PLACEHOLDER_VITE_SCRIPT_SOURCE", `"/${sourceName}"`)
                .trim();
            // note bootstrap.js doesn't need to be hashed because it will be injected directly
            // into served html pages
            await fs.writeFile(path.join(distDir, "bootstrap.js"), bootstrapJs);

            const metadataRs = `
/// Generated metadata about JS bundle - do no edit manually
pub static JS_SOURCEMAP_PATH: &str = "/${name}";
pub static JS_SOURCEMAP: &str = include_str!("${name}");
pub static JS_SOURCE: &str = include_str!("${sourceName}");
pub static JS_BOOTSTRAP: &str = concat!("<script>",include_str!("bootstrap.js"),"</script>");
`;
            await fs.writeFile(path.join(distDir, "metadata.rs"), metadataRs);
        },
    };
};

export default configure(async () => {
    await writeIndexHtml();
    return {
        define: {
            BIN_NAME: "'shwoop'",
        },
        plugins: [postProcess()],
        build: {
            rolldownOptions: {
                output: {
                    entryFileNames: JS_CHUNK_NAME + "-xxx-[hash].js",
                },
            },
        },
    };
});
