import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

const version = "8.0.1";
const sourceHash = "05ee0b03119b45c0bdb4df654b96802e909e0a752f72e4fe3794f487229e5a41";
const makeHash = "af0bdba17f06fe037f0194069adaa31a8fe45f1a11381501896aea1fae37bd5d";
const options = [
  "--toolchain=msvc", "--arch=x86_64", "--disable-everything",
  "--disable-autodetect", "--disable-network", "--disable-doc", "--disable-debug",
  "--disable-x86asm", "--enable-small", "--disable-ffplay", "--disable-ffprobe",
  "--disable-avdevice", "--disable-swscale", "--enable-protocol=file",
  "--enable-demuxer=xwma", "--enable-decoder=wmav2,wmapro",
  "--enable-encoder=adpcm_ms", "--enable-muxer=wav",
  "--enable-filter=aresample,aformat,anull,asetpts", "--enable-swresample", "--enable-ffmpeg",
];

function run(command, args, settings = {}) {
  const result = spawnSync(command, args, { stdio: "inherit", ...settings });
  if (result.error || result.status !== 0) {
    throw new Error(`Audio converter build failed: ${result.error?.message || command}`);
  }
  return result;
}

async function download(url, path, expectedHash) {
  if (!existsSync(path)) {
    const response = await fetch(url);
    if (!response.ok) {
      throw new Error(`Cannot download audio build source: HTTP ${response.status}`);
    }
    writeFileSync(path, Buffer.from(await response.arrayBuffer()));
  }
  const hash = createHash("sha256").update(readFileSync(path)).digest("hex");
  if (hash !== expectedHash) {
    throw new Error(`Audio build download failed verification: ${path}`);
  }
}

export async function prepareAudio(root) {
  const binary = resolve(root, "engine/waw-audio.exe");
  const stamp = resolve(root, ".local/audio-fingerprint");
  const fingerprint = createHash("sha256")
    .update(sourceHash).update(makeHash).update(options.join(" ")).digest("hex");
  if (existsSync(binary) && existsSync(stamp) && readFileSync(stamp, "utf8") === fingerprint) {
    return;
  }

  console.log("Building the optional WAV converter. The first build takes a few minutes.");
  const build = resolve(root, ".local/audio-build");
  mkdirSync(build, { recursive: true });
  const archive = resolve(build, "ffmpeg.tar.xz");
  const makeArchive = resolve(build, "make.tar.zst");
  await download(`https://ffmpeg.org/releases/ffmpeg-${version}.tar.xz`, archive, sourceHash);
  await download("https://repo.msys2.org/msys/x86_64/make-4.4.1-3-x86_64.pkg.tar.zst", makeArchive, makeHash);
  const source = resolve(build, `ffmpeg-${version}`);
  if (!existsSync(resolve(source, "configure"))) {
    run("tar", ["-xf", archive, "-C", build]);
  }
  if (!existsSync(resolve(build, "usr/bin/make.exe"))) {
    run("tar", ["-xf", makeArchive, "-C", build]);
  }

  const where = spawnSync("where.exe", ["git"], { encoding: "utf8" });
  const git = where.stdout?.trim().split(/\r?\n/)[0];
  const gitRoot = git ? resolve(dirname(git), "..") : "";
  const bash = resolve(gitRoot, "bin/bash.exe");
  if (!git || !existsSync(bash)) {
    throw new Error("Install Git for Windows, then reopen your terminal.");
  }
  const locator = resolve(process.env["ProgramFiles(x86)"] || "C:/Program Files (x86)", "Microsoft Visual Studio/Installer/vswhere.exe");
  const found = spawnSync(locator, ["-latest", "-products", "*", "-requires", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath"], { encoding: "utf8" });
  const installation = found.stdout?.trim();
  if (!installation) {
    throw new Error("Install Visual Studio Build Tools with Desktop development with C++.");
  }

  const script = resolve(build, "build.sh");
  writeFileSync(script, `#!/usr/bin/env bash\nset -eu\ncd "$(dirname "$0")/ffmpeg-${version}"\n./configure ${options.join(" ")}\n../usr/bin/make.exe -j4 ffmpeg.exe\n`);
  const batch = resolve(build, "build.cmd");
  writeFileSync(batch, '@echo off\r\ncall "%WAW_AUDIO_VCVARS%"\r\nif errorlevel 1 exit /b 1\r\nset "PATH=%PATH%;%WAW_AUDIO_UNIX%"\r\n"%WAW_AUDIO_BASH%" "%WAW_AUDIO_SCRIPT%"\r\n');
  // Paths travel through environment variables, not shell-generated arguments.
  run("cmd.exe", ["/d", "/c", "build.cmd"], {
    cwd: build,
    env: {
      ...process.env,
      WAW_AUDIO_VCVARS: resolve(installation, "VC/Auxiliary/Build/vcvars64.bat"),
      WAW_AUDIO_UNIX: resolve(gitRoot, "usr/bin"),
      WAW_AUDIO_BASH: bash,
      WAW_AUDIO_SCRIPT: script.replaceAll("\\", "/"),
    },
  });
  mkdirSync(resolve(root, "engine"), { recursive: true });
  copyFileSync(resolve(source, "ffmpeg.exe"), binary);
  writeFileSync(stamp, fingerprint);
}
