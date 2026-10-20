# WaW FF Extractor

An unofficial community tool by **phil81334**. Not affiliated with Treyarch, Activision, UGX or Tom_Bmx.

Extract **raw files, embedded sounds and CSV tables** from PC World at War `.ff` files. Inspired by Tom’s extractor, with a new interface and OpenAssetTools underneath.

Raw files and sounds keep their stored bytes. CSV tables keep their stored cells, with recreated CSV formatting. No map, model, FX or menu reconstruction. No Modtools installation required.

![WaW FF Extractor](images/waw-ff-extractor.png)

[Extraction result screenshot](images/waw-ff-extractor-extraction-result.png)

## Use it

Download **[waw-ff-extractor.exe](https://github.com/phil81334/waw-ff-extractor/releases/latest/download/waw-ff-extractor.exe)** from [Releases](https://github.com/phil81334/waw-ff-extractor/releases/latest).

Put it beside `CoDWaW.exe` and run it. **Add files** opens `zone/english`. Drop FF files onto it and click **Extract**.

Needs Windows 10/11, [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) and the [Microsoft Visual C++ x86 Runtime](https://aka.ms/vs/17/release/vc_redist.x86.exe). These may already be installed.

Each FF gets its own output folder beside it. You can choose another location. Existing files are skipped unless you choose **Replace** or **Back up folder, then replace**. This renames the whole old output folder to `<FF name>.backup-1` (then `.backup-2`, etc.) and creates a fresh output folder.

**Skip** leaves matching paths untouched without comparing contents. **Replace** also leaves unrelated old files behind. Switching from another extractor? Choose **Back up folder, then replace** for clean output; no manual deletion needed.

Each successful extraction saves `waw-ff-extractor-log.txt` (numbered if already present). Extracted files named after the FF may contain old compiler errors stored in the FF itself—not errors from our app.

Only embedded sounds are extracted. **Convert XWMA to WAV copies** is optional and off by default. It keeps the originals and adds 44.1 kHz, stereo, 4-bit Microsoft ADPCM WAVs under `converted-wav/sound/...`, following [this WaW sound guide](https://wiki.zeroy.com/index.php/Call_of_Duty_5:_Sounds). The format is checked automatically; in-game use has not been tested.

## Run from source

Built with JavaScript, Vite and Tauri (Rust). Windows only for the desktop app.

1. Install [Node.js 22.12 or newer](https://nodejs.org/), [Git](https://git-scm.com/downloads/win), and [Rust](https://rustup.rs/).
2. Install [Visual Studio 2022 Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022). Select **Desktop development with C++**, including a Windows SDK.
3. Download and unzip this repo, or clone it. Open its folder in VS Code.
4. Open **Terminal → New Terminal** and run `npm install`, then `npm run dev`.

Development runs do not need to be inside the game folder. Set `WAW_ROOT` to your game folder to start Add files there.

The first desktop run downloads and builds the extraction engine and small audio converter. Later runs reuse it. Windows also needs the [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/), normally already installed.

These commands also appear in VS Code’s **NPM Scripts** panel:

| Command             | What it does                                               |
| ------------------- | ---------------------------------------------------------- |
| `npm run dev`       | Opens the desktop app while you work on it.                |
| `npm run dev:web`   | Previews the interface in a browser. Cannot extract files. |
| `npm run build`     | Creates `release/waw-ff-extractor.exe`.                    |
| `npm run build:web` | Builds only the interface. Does not create an EXE.         |

[Credits and engine source](ATTRIBUTION.md) · [GPL-3.0 licence](LICENSE)
