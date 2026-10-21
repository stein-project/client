# Contributing to Stein Client

Read the [Stein contributing guide](https://github.com/Stein-Project/.github/blob/main/CONTRIBUTING.md) first: it
covers the rules, commits and the [CLA](https://github.com/Stein-Project/.github/blob/main/CLA.md) your first pull
request asks you to sign. This page is what is specific to the launcher.

## Build and check

```sh
cd ui && npm ci && npm run build && cd ..   # tsc --noEmit and the Vite build
cargo fmt --all --check                     # rustfmt.toml; `cargo fmt --all` fixes it
cargo test --workspace                      # what the CI runs, on Windows
```

Then run `target/debug/SteinClient.exe` (or `cargo run -p stein-client`). For changes to the interface only,
`npm run dev` in `ui/` shows it in a browser with sample data, without Tauri.

Debug builds never update themselves, so your `target/debug` executable is never swapped for the latest release.
They do use your real folders, though: the game folder is `%APPDATA%\.steinclient` and the accounts are in
`%APPDATA%\.steinauth` (plus the system vault). To keep test accounts apart, set `STEIN_AUTH_DIR` to another folder
before starting the launcher (see `auth/src/lib.rs`). To test the core without touching your own install, use the
command line, which takes a separate folder: `cargo run -p stein-core --bin stein-cli -- --dir <folder> help`.

## Rules specific to the launcher

- **Nothing from Mojang in the launcher.** Minecraft is downloaded from Mojang's own servers onto the player's
  machine, as the official launcher does, and only starts with an account that owns Minecraft: Java Edition.
- **Every download is checked** against a sha1 or sha256 that comes from a source the launcher already trusts.
- **Few options on purpose.** The launcher comes set up; a new setting needs a good reason.
- **Accounts stay in the system vault.** Refresh tokens never go to a file or a log; the command line prints the
  access token as `***`.
- **The interface talks to Rust only through `ui/src/api.ts`.** A new Tauri command goes in `launcher/` and in
  `launcher/capabilities/default.json` if it needs a permission.

## Releases

Write `changelog/<version>.en_US.md` (and optionally `.pt_BR.md`), bump `version` in `Cargo.toml`
(`[workspace.package]`), then push a `v<version>` tag. `.github/workflows/release.yml` builds `SteinClient.exe` and
publishes it with `update.json` and `THIRD-PARTY-NOTICES.txt` as a release of this repository. Running the workflow
by hand is a dry run that publishes nothing. After the release, run the `publish` workflow of the
[Stein registry](https://github.com/Stein-Project/registry) by hand: it publishes the same content, signed, as
`client.json`, where launchers after 0.3.2 look for updates; 0.3.1 and 0.3.2 keep reading the release's `update.json`.

Everything else the launcher looks up at run time (the Loader version, the official mods and which are enabled out
of the box, links, the game's Java arguments, revoked mods, migrations) comes from the registry, signed: see its
README for the formats. To test against a local copy, set `STEIN_REGISTRY` to its address (an empty value fetches
nothing and uses the stored copies in `stein-client/registry/`).

Up to 0.3.0 the releases went to [Stein-Project/stein-client](https://github.com/Stein-Project/stein-client), which is
frozen. Launchers 0.3.0 still read its `update.json`; when the first release here came out, the `update.json` of its
v0.3.0 release was replaced by the new one (whose `base` points here), so they update to it and read this repository
from then on. Leave it as it is.

When `Cargo.lock` or `ui/package-lock.json` changes, regenerate `THIRD-PARTY-NOTICES.txt` (the licences of everything
inside `SteinClient.exe`, published with each release) and commit it:

```sh
cargo install cargo-about --locked --features cli   # once
tools/notices.sh                                    # needs npm ci in ui/ first; reads about.toml and tools/notices.hbs
```
