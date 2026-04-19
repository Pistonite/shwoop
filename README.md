# shwoop

A development server for static web content with hot reload and optional build integration.
Perfect for developing static-content and documentation generator like `rustdoc`, `mdbook`, `typedoc`, etc.

This tool serves files from a directory, watches for changes in output and source,
and builds the website and updates connected browsers when a change is detected.
It aims to match the experience of Hot Module Reloading (HMR), meaning only the pargs that changed
in a page will be reloaded instead of a full page reload, resulting in a smoother development experience.

Usually, the web frameworks are able to support HMR because they understand how the full app is built.
To achieve this for *any* static website, this tool injects a client that replaces the page
with an `<iframe>` that serves the same page. When reloading, it creates a new `<iframe>` and loads
the content in the background, restoring state, and swaps the frames when the new frame is fully loaded.

## Installation
To try out the tool, the easiest way is to use [`cargo-binstall`](https://github.com/cargo-bins/cargo-binstall)
(which can be installed without `cargo` or Rust toolchain). It sources the binaries
from GitHub releases. Alternatively you can download the binaries from GitHub release directly.
```
cargo-binstall shwoop --only-signed
```

To build from Rust source + published JS assets (requires Rust)
```
cargo install shwoop
```

Building from only sources in this repo requires Rust, NodeJS and PNPM
```
pnpm install
pnpm exec monolint --config
pnpm exec vite build
cargo build --release
```

## Usage

```
shwoop <PATH> [OPTIONS] [-- COMMAND...]
```

`PATH` is the output directory (or file) to serve. If a build command is provided, shwoop runs it
whenever watched files change before reloading connected browsers.
`-w/--watch` switches can specify additional source directories to watch
that will trigger the build command when changed. `PATH` will not trigger build command.

For more options, see `shwoop --help`

### Examples

Serve and watch `dist` as output directory with no build step:
```
shwoop dist
```

Build documentation with `cargo` and serve, rebuild and reload whenever
`src` changes.
```
shwoop target/doc -w src -- cargo doc
```

Plain file server with no reload machinery

```
shwoop dist --raw
```

