import "./style.css";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";

const element = (id) => document.getElementById(id);
const files = [];
let outputRoot = null;
let busy = false;
let zoneDirectory;
const desktop = isTauri();

element("environment").textContent = import.meta.env.DEV
  ? "Development"
  : "Production";

function message(text, error = false, success = false) {
  element("message").textContent = text;
  element("message").classList.toggle("error", error);
  element("message").classList.toggle("success", success && !error);
  if (error) {
    element("status").textContent = "Needs attention";
  }
}

function refresh() {
  const queue = element("queue");
  queue.replaceChildren();

  for (const file of files) {
    const row = document.createElement("li");
    const name = document.createElement("span");
    name.textContent = file.path.split(/[\\/]/).pop();
    name.title = file.path;
    const status = document.createElement("span");
    status.textContent = file.status;
    row.append(name, status);
    queue.append(row);
  }

  element("empty").hidden = files.length > 0;
  element("count").textContent =
    `${files.length} file${files.length === 1 ? "" : "s"} added`;
  element("extract").disabled = busy || files.length === 0 || !desktop;

  for (const id of [
    "add",
    "clear",
    "browse-output",
    "reset-output",
    "overwrite",
    "raw",
    "sounds",
    "tables",
  ]) {
    element(id).disabled = busy || !desktop;
  }
  element("convert-sounds").disabled = busy || !desktop || !element("sounds").checked;
}

element("sounds").addEventListener("change", refresh);

function addFiles(paths) {
  if (busy) {
    return;
  }

  let rejected = 0;
  for (const path of paths) {
    if (!path.toLowerCase().endsWith(".ff")) {
      rejected += 1;
      continue;
    }
    if (!files.some((file) => file.path.toLowerCase() === path.toLowerCase())) {
      files.push({ path, status: "Ready" });
    }
  }

  refresh();
  if (paths.length > 0 && rejected === 0) {
    element("status").textContent = "Ready";
  }
  if (rejected > 0) {
    message("Only .ff files can be added.", true);
  }
}

element("add").addEventListener("click", async () => {
  try {
    const paths = await open({
      multiple: true,
      defaultPath: zoneDirectory,
      filters: [{ name: "World at War FastFiles", extensions: ["ff"] }],
    });
    if (paths) {
      addFiles(Array.isArray(paths) ? paths : [paths]);
    }
  } catch (error) {
    message(String(error), true);
  }
});

element("clear").addEventListener("click", () => {
  files.length = 0;
  element("log").textContent = "";
  message("Ready.");
  element("status").textContent = "Ready";
  refresh();
});

element("browse-output").addEventListener("click", async () => {
  try {
    const path = await open({ directory: true, multiple: false });
    if (path) {
      outputRoot = path;
      element("destination").textContent = `${path} (one folder per FF)`;
    }
  } catch (error) {
    message(String(error), true);
  }
});

element("reset-output").addEventListener("click", () => {
  outputRoot = null;
  element("destination").textContent =
    "A folder beside each FF, named after that FF";
});

element("extract").addEventListener("click", async () => {
  const types = [];
  for (const [id, type] of [
    ["raw", "rawfile"],
    ["sounds", "loadedsound"],
    ["tables", "stringtable"],
  ]) {
    if (element(id).checked) {
      types.push(type);
    }
  }
  if (types.length === 0) {
    message("Choose at least one type to extract.", true);
    return;
  }

  busy = true;
  element("status").textContent = "Extracting…";
  refresh();
  let failures = 0;
  let written = 0;
  let skipped = 0;
  element("log").textContent = "";

  // Keep the queue sequential: two FFs must never write into the same folder at once.
  for (const file of files) {
    file.status = "Extracting…";
    message(`Extracting ${file.path.split(/[\\/]/).pop()}…`);
    refresh();

    try {
      const result = await invoke("extract", {
        request: {
          input: file.path,
          outputRoot,
          types,
          overwrite: element("overwrite").value,
          convertSounds: element("sounds").checked && element("convert-sounds").checked,
        },
      });
      written += result.written;
      skipped += result.skipped;
      file.status = `${result.written} saved · ${result.skipped} skipped`;
      element("log").textContent +=
        `${file.path}\n${result.log}\nSaved to: ${result.destination}\n\n`;
    } catch (error) {
      failures += 1;
      file.status = "Failed — see log";
      element("log").textContent += `${file.path}\n${String(error)}\n\n`;
      element("log").parentElement.open = true;
    }
    refresh();
  }

  busy = false;
  refresh();
  message(
    `Finished: ${written} saved, ${skipped} skipped, ${failures} FFs failed.`,
    failures > 0,
    failures === 0,
  );
  element("status").textContent = failures > 0 ? "Done with errors" : "Done";
});

element("about").addEventListener("click", () => {
  element("about-dialog").showModal();
  element("repo-url").focus();
  element("repo-url").select();
});
element("close-about").addEventListener("click", () =>
  element("about-dialog").close(),
);

if (desktop) {
  getCurrentWebview()
    .onDragDropEvent((event) => {
      element("drop-zone").classList.toggle(
        "dragging",
        event.payload.type === "over",
      );
      if (event.payload.type === "drop") {
        addFiles(event.payload.paths);
      }
    })
    .catch((error) => message(`Drag and drop unavailable: ${error}`, true));
} else {
  element("status").textContent = "UI preview";
  message(
    "Browser preview only. Run npm run dev to extract files in the desktop app.",
  );
}

refresh();

element("repo-url").addEventListener("click", (event) => {
  event.target.select();
});
element("startup-error").addEventListener("cancel", (event) => {
  event.preventDefault();
});
element("quit").addEventListener("click", () => {
  invoke("quit");
});

async function start() {
  try {
    if (desktop) {
      zoneDirectory = await invoke("zone_directory");
    }
    element("app").hidden = false;
    element("footer").hidden = false;
  } catch (error) {
    element("startup-message").textContent = String(error);
    element("startup-error").showModal();
  }
}

start();

element("copy-log").addEventListener("click", async () => {
  try {
    await navigator.clipboard.writeText(element("log").textContent);
    message("Log copied.", false, true);
  } catch (error) {
    message(`Could not copy the log: ${error}`, true);
  }
});
