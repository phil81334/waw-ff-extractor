# Attribution

- **phil81334** — WaW FF Extractor interface and desktop app.
- **Laupetin and OpenAssetTools contributors** — the extraction engine, licensed under GPL-3.0. [OpenAssetTools source](https://github.com/Laupetin/OpenAssetTools/tree/30d2d498bd8e3f2401037b24f3db6f96d39554c4).
- **FFmpeg contributors** — optional XWMA-to-ADPCM converter, built from [FFmpeg 8.0.1](https://ffmpeg.org/releases/ffmpeg-8.0.1.tar.xz) under LGPL-2.1-or-later. `scripts/audio.mjs` records the verified source download and build options; no third-party codec libraries are enabled.
- **Tom_Bmx** — the original FastFile Extractor that inspired this project. None of its executable or source is included.

The engine is built from commit `30d2d498bd8e3f2401037b24f3db6f96d39554c4` and its pinned dependencies. `engine/safety.patch` restricts output paths, removes the generated zone inventory, and disables external search paths for embedded-only exports. The raw-file, embedded-sound and CSV exporters are unchanged.

`scripts/engine.mjs` downloads and builds that source when you run the desktop app from source. The complete checkout stays in `.local/engine-source`, including dependency licences. Research is separate and never packaged.

Each release includes the matching engine source and dependencies in `openassettools-source.tar.gz`. The tagged app source includes its build scripts, dependency lockfiles and engine patch. No game assets are bundled. Extracted game files retain their original ownership.

The optional converter is a separate executable embedded in the app. Its unmodified FFmpeg source and licence are in `.local/audio-build/ffmpeg-8.0.1` after building. Releases containing the converter include its source and licence in `ffmpeg-8.0.1-source.tar.xz`.
