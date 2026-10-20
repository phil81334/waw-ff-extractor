import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { createHash } from "node:crypto";
import { resolve } from "node:path";

const commit = "30d2d498bd8e3f2401037b24f3db6f96d39554c4";

function run(command, args, cwd) {
  const result = spawnSync(command, args, { cwd, stdio: "inherit" });
  if (result.error || result.status !== 0) {
    throw new Error(
      `${command} failed. ${result.error?.message || "See the output above."}`,
    );
  }
}

export async function prepareEngine(root) {
  const binary = resolve(root, "engine/Unlinker.exe");
  const patch = resolve(root, "engine/safety.patch");
  const fingerprint = createHash("sha256")
    .update(commit)
    .update(readFileSync(patch))
    .digest("hex");
  const stamp = resolve(root, ".local/engine-fingerprint");

  if (
    existsSync(binary) &&
    existsSync(stamp) &&
    readFileSync(stamp, "utf8") === fingerprint
  ) {
    return;
  }
  if (process.platform !== "win32") {
    throw new Error(
      "Desktop builds currently require Windows. npm run dev:web works in a browser.",
    );
  }

  console.log(
    "Preparing OpenAssetTools. The first build downloads its source and may take several minutes.",
  );
  const source = resolve(root, ".local/engine-source");
  mkdirSync(resolve(root, ".local"), { recursive: true });
  if (!existsSync(resolve(source, ".git"))) {
    run(
      "git",
      ["clone", "https://github.com/Laupetin/OpenAssetTools.git", source],
      root,
    );
    run("git", ["checkout", "--detach", commit], source);
    run("git", ["submodule", "update", "--init", "--recursive"], source);
  }

  const revision = spawnSync("git", ["rev-parse", "HEAD"], {
    cwd: source,
    encoding: "utf8",
  });
  if (revision.status !== 0 || revision.stdout.trim() !== commit) {
    throw new Error(
      `The engine source must be at ${commit}. Existing source was left untouched.`,
    );
  }

  // An already-patched checkout is fine; never reset somebody's source edits.
  const applied = spawnSync("git", ["apply", "--reverse", "--check", patch], {
    cwd: source,
    stdio: "ignore",
  });
  if (applied.status !== 0) {
    run("git", ["apply", patch], source);
  }

  const premakeFolder = resolve(root, ".local/premake");
  const premake = resolve(premakeFolder, "premake5.exe");
  if (!existsSync(premake)) {
    mkdirSync(premakeFolder, { recursive: true });
    const response = await fetch(
      "https://github.com/premake/premake-core/releases/download/v5.0.0-beta8/premake-5.0.0-beta8-windows.zip",
    );
    if (!response.ok) {
      throw new Error(`Cannot download Premake: HTTP ${response.status}`);
    }
    const archive = resolve(premakeFolder, "premake.zip");
    writeFileSync(archive, Buffer.from(await response.arrayBuffer()));
    // Paths are passed as environment variables, not interpolated into shell code.
    const expanded = spawnSync(
      "powershell.exe",
      [
        "-NoProfile",
        "-Command",
        "Expand-Archive -LiteralPath $env:WAW_PREMAKE_ZIP -DestinationPath $env:WAW_PREMAKE_DIR -Force",
      ],
      {
        env: {
          ...process.env,
          WAW_PREMAKE_ZIP: archive,
          WAW_PREMAKE_DIR: premakeFolder,
        },
        stdio: "inherit",
      },
    );
    if (expanded.status !== 0) {
      throw new Error("Could not unpack Premake.");
    }
  }

  const premakeHash = createHash("sha256")
    .update(readFileSync(premake))
    .digest("hex");
  if (
    premakeHash !==
    "2301e3e23ff3074cb83a5ea6103d68c7ea81dad56b786807c84b0643cddea31b"
  ) {
    throw new Error("Premake did not match the expected download.");
  }

  run(premake, ["--oat-version=waw-ff-extractor-0.1.0", "vs2022"], source);
  const locator = resolve(
    process.env["ProgramFiles(x86)"] || "C:/Program Files (x86)",
    "Microsoft Visual Studio/Installer/vswhere.exe",
  );
  const found = spawnSync(
    locator,
    [
      "-latest",
      "-products",
      "*",
      "-requires",
      "Microsoft.Component.MSBuild",
      "-find",
      "MSBuild/**/Bin/MSBuild.exe",
    ],
    { encoding: "utf8" },
  );
  const msbuild = found.stdout?.trim().split(/\r?\n/)[0];
  if (found.status !== 0 || !msbuild) {
    throw new Error(
      "Install Visual Studio Build Tools with Desktop development with C++. See README.md.",
    );
  }

  run(
    msbuild,
    [
      "build/OpenAssetTools.sln",
      "/t:Tools\\UnlinkerCli",
      "/p:Configuration=Release",
      "/p:Platform=Win32",
      "/p:CL_MPCount=2",
      "/m:2",
      "/v:minimal",
    ],
    source,
  );
  mkdirSync(resolve(root, "engine"), { recursive: true });
  copyFileSync(resolve(source, "build/bin/Release_x86/Unlinker.exe"), binary);
  writeFileSync(stamp, fingerprint);
}
