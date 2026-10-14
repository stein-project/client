# Stein Client

The launcher for **original Minecraft 1.8.9** with the Stein Loader: lighter, with the official Stein mods, ready to
play. Few options on purpose — it comes set up.

**[Download SteinClient.exe](https://github.com/Stein-Project/client/releases/latest/download/SteinClient.exe)**, keep
it wherever you like (the desktop is a good place) and run it. Sign in with the Microsoft account that owns Minecraft:
Java Edition and click **Play**: the launcher downloads Minecraft 1.8.9 from Mojang, the Stein Loader, Java and the
official mods into `%APPDATA%\.steinclient`. Every time it opens, it checks for a new version of itself and updates
before starting.

**Original Minecraft only.** The game files come from Mojang, straight to your computer, and the game only starts once
an account that owns Minecraft: Java Edition is in the launcher. With one, you can also add offline nicknames for
singleplayer and servers without authentication. Nothing from Mojang is redistributed here.

> Stein is not an official Minecraft product. It is not approved by or associated with Mojang or Microsoft.

## Privacy

The Stein Client sends nothing about you anywhere. It talks only to the services it needs to work:

- **Microsoft, Xbox Live and Minecraft** (`login.live.com`, `xboxlive.com`, `minecraftservices.com`): signing in and
  confirming that the account owns the game. The sign-in token stays in Windows' own credential vault.
- **Mojang** (`mojang.com`, `minecraft.net`): Minecraft 1.8.9 itself.
- **GitHub** (`github.com`, `githubusercontent.com`): the Stein Loader, Java, the official mods and the launcher's own
  updates.
- **Third-party mod sources you add**: wherever their author hosts them (only after you confirm the warning).
- **mc-heads.net**: the skin heads shown next to your accounts. It receives each account's uuid, which is how it
  knows whose head to draw.

No analytics, no telemetry, no ads.

## Uninstalling

Delete `SteinClient.exe` and the folder `%APPDATA%\.steinclient` (worlds, settings and mods live there: keep a copy of
your `saves` first if you want them). Saved Microsoft accounts are in `%APPDATA%\.steinauth` and in Windows' Credential
Manager (entries ending in `.SteinAuth`).

## Building

You need [Rust](https://rustup.rs) (stable) and [Node.js](https://nodejs.org) 20 or newer. The interface is built
first, because it goes inside the executable:

```sh
cd ui && npm ci && npm run build && cd ..
cargo build --release -p stein-client   # target/release/SteinClient.exe
cargo test --workspace                  # what the CI runs
tools/client.sh                         # the release build: build/client/SteinClient.exe and update.json
```

`tools/client.sh` is a bash script and needs `sha256sum`: on Windows, run it from Git Bash.

To work on the interface alone, `npm run dev` in `ui/` opens it in a browser with sample data (`ui/src/preview.ts`
lists the query parameters that pick a screen). `cargo run -p stein-core --bin stein-cli -- help` drives the core
from the command line, with no interface.

## Layout

| Path | What it is |
|---|---|
| `core/` | `stein-core`: folders, checked downloads, Minecraft 1.8.9 from Mojang, the Stein Loader release, mod sources, starting the game and the launcher's own updates |
| `auth/` | `stein-client-auth`: Microsoft account sign-in (device code → Xbox Live → Minecraft) with the refresh token in the system credential vault |
| `launcher/` | `SteinClient.exe`: the Tauri app and the commands the interface calls |
| `ui/` | the interface (React + Vite + TypeScript) and its design system (`ui/src/ds`) |
| `changelog/` | the release notes of each version |
| `tools/` | the release build (`client.sh`), the third-party notices (`notices.sh`) and the Windows icon (`MakeIco.java`) |

## Contributing

Bug reports, ideas and pull requests are welcome. Read the
[contributing guide](https://github.com/Stein-Project/.github/blob/main/CONTRIBUTING.md) first; your first pull
request asks you to sign the [CLA](https://github.com/Stein-Project/.github/blob/main/CLA.md). What is specific to the
launcher is in [CONTRIBUTING.md](https://github.com/Stein-Project/client/blob/main/CONTRIBUTING.md).

## Licence

Stein Client is licensed under the [Mozilla Public License 2.0](https://github.com/Stein-Project/client/blob/main/LICENSE). Third-party software and its licences are
in [NOTICE](https://github.com/Stein-Project/client/blob/main/NOTICE) and [THIRD-PARTY-NOTICES.txt](https://github.com/Stein-Project/client/blob/main/THIRD-PARTY-NOTICES.txt). The Stein name and logo are not covered by the licence: see the
[trademark policy](https://github.com/Stein-Project/.github/blob/main/TRADEMARK.md).

Created by [Breno Xavier](https://github.com/brenoxavier).
