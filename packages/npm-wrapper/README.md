# hardless (npm)

`npx hardless` runs the [Hardless](https://github.com/hardlessdev/hardless) POS hardware
emulator without a global install.

```sh
npx hardless start
```

On first run it downloads the `hardless` binary for your platform from
[GitHub Releases](https://github.com/hardlessdev/hardless/releases), checks its SHA-256 against
the release's checksum file and caches it. Later runs start straight away. The package has no
dependencies.

Supported: Windows, macOS and Linux on x86_64 and arm64 (NFR-11). Linux uses the static musl
build, so it runs on any distribution.

| Environment variable | Effect                                                                                                             |
| -------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `HARDLESS_CACHE_DIR` | Where binaries are cached (default: `%LOCALAPPDATA%\hardless`, `~/Library/Caches/hardless` or `~/.cache/hardless`) |
| `HARDLESS_BINARY`    | Run this binary instead of downloading one                                                                         |

Other ways to install: the shell and PowerShell installers and Homebrew (see the release page),
`cargo install hardless-cli`, or the Docker image `hardless/hardless`.

Apache-2.0.
