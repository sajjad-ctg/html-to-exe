import { useEffect, useState } from "preact/hooks";
import { Channel, invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import "./App.css";

const THEME_KEY = "htx-theme";

function initialTheme() {
  const stored = localStorage.getItem(THEME_KEY);
  if (stored === "light" || stored === "dark") return stored;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function formatBytes(bytes) {
  if (!bytes) return "empty";
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function prettyName(path) {
  const norm = path.replace(/\\/g, "/");
  const base = norm.split("/").pop() || path;
  return base.replace(/\.zip$/i, "");
}

function App() {
  const [theme, setTheme] = useState(initialTheme);
  const [zipPath, setZipPath] = useState("");
  const [zipInfo, setZipInfo] = useState(null); // null | {file_count, total_bytes, entry}
  const [inspectError, setInspectError] = useState("");
  const [title, setTitle] = useState("");
  const [iconPath, setIconPath] = useState("");
  const [mode, setMode] = useState("single"); // single | folder
  const [webview2, setWebview2] = useState("system"); // system | autoheal
  const [status, setStatus] = useState("idle"); // idle | building | done | error
  const [statusDetail, setStatusDetail] = useState("");
  const [resultPath, setResultPath] = useState("");

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem(THEME_KEY, theme);
  }, [theme]);

  async function loadZip(picked) {
    setZipPath(picked);
    setZipInfo(null);
    setInspectError("");
    setTitle(prettyName(picked));
    setResultPath("");
    setStatus("idle");
    setStatusDetail("");
    try {
      const info = await invoke("inspect_zip", { path: picked });
      setZipInfo(info);
    } catch (error) {
      setInspectError(String(error));
    }
  }

  async function pickZip() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "HTML5 build (zip)", extensions: ["zip"] }],
    });
    if (picked) loadZip(picked);
  }

  // File drops from Explorer: with dragDropEnabled (Tauri default) the
  // WebView2 native handler owns drags, so plain HTML5 drop events never
  // fire here — listen through the webview API instead.
  useEffect(() => {
    let unlisten = () => {};
    getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type !== "drop") return;
      const zip = (event.payload.paths || []).find((p) => p.toLowerCase().endsWith(".zip"));
      if (zip) loadZip(zip);
      else setStatusDetail("dropped files ignored — drop a .zip");
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten();
  }, []);

  async function pickIcon() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "Windows icon (ico / png)", extensions: ["ico", "png"] }],
    });
    if (picked) setIconPath(picked);
  }

  async function build() {
    setStatus("building");
    setStatusDetail("starting…");
    try {
      // Progress must travel through a Tauri Channel (a plain callback
      // is not serializable across IPC and would arrive as "missing").
      const progress = new Channel();
      progress.onmessage = (message) => setStatusDetail(message);
      const result = await invoke("export", {
        zipPath,
        title: title.trim(),
        iconPath: iconPath || null,
        mode,
        webview2,
        onProgress: progress,
      });
      setStatus("done");
      setStatusDetail(result.path);
      setResultPath(result.path);
    } catch (error) {
      setStatus("error");
      setStatusDetail(String(error));
    }
  }

  const canBuild = Boolean(zipPath) && Boolean(zipInfo?.entry) && title.trim().length > 0 && !resultPath;

  return (
    <main class="app">
      <header class="topbar">
        <span class="brand">
          html<span class="brand-dim">-to-</span>exe
        </span>
        <button
          class="theme-toggle"
          onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
          aria-label="Toggle color theme"
          title="Toggle color theme"
        >
          {theme === "dark" ? "☀" : "☾"}
        </button>
      </header>

      <section class="card">
        {/* zip picker */}
        <button class={`dropzone ${zipPath ? "picked" : ""}`} onClick={pickZip}>
          {zipPath ? (
            <>
              <span class="dropzone-name">{zipPath.split(/[\\/]/).pop()}</span>
              <span class="dropzone-meta">
                {zipInfo
                  ? `${zipInfo.file_count} files · ${formatBytes(zipInfo.total_bytes)}`
                  : "reading…"}
              </span>
              {inspectError && <span class="bad">{inspectError}</span>}
              {zipInfo && !zipInfo.entry && (
                <span class="bad">index.html not found in the archive</span>
              )}
              {zipInfo?.entry && <span class="ok">entry: {zipInfo.entry}</span>}
            </>
          ) : (
            <>
              <span class="dropzone-big">Click to choose a .zip — or drag one here</span>
              <span class="dropzone-meta">any HTML5 game or app build</span>
            </>
          )}
        </button>

        {/* title */}
        <label class="field">
          <span class="field-label">Game title</span>
          <input
            type="text"
            value={title}
            placeholder="My Game"
            onInput={(e) => setTitle(e.currentTarget.value)}
            maxLength={60}
          />
        </label>

        {/* icon */}
        <div class="field-row">
          <label class="field grow">
            <span class="field-label">Icon (.png / .ico, optional)</span>
            <button class="pick" onClick={pickIcon} title={iconPath}>
              {iconPath ? iconPath.split(/[\\/]/).pop() : "choose file…"}
            </button>
          </label>
          {iconPath && (
            <button class="icon-clear" onClick={() => setIconPath("")} title="Remove icon">
              ✕
            </button>
          )}
        </div>

        {/* export mode */}
        <div class="field">
          <span class="field-label">Output</span>
          <div class="segmented">
            <button
              class={mode === "single" ? "seg active" : "seg"}
              onClick={() => setMode("single")}
            >
              Single .exe
            </button>
            <button
              class={mode === "folder" ? "seg active" : "seg"}
              onClick={() => setMode("folder")}
            >
              Folder
            </button>
          </div>
        </div>

        {/* WebView2 runtime policy */}
        <div class="field">
          <span class="field-label">WebView2 runtime</span>
          <div class="segmented">
            <button
              class={webview2 === "system" ? "seg active" : "seg"}
              onClick={() => setWebview2("system")}
              title="Rely on the WebView2 runtime already installed on the player's Windows (default; almost never missing)"
            >
              System (default)
            </button>
            <button
              class={webview2 === "autoheal" ? "seg active" : "seg"}
              onClick={() => setWebview2("autoheal")}
              title="Embed Microsoft's official bootstrapper (~2 MB): if a player's Windows is missing the runtime, the game offers a one-time silent install instead of failing"
            >
              Auto-heal (+~2 MB)
            </button>
          </div>
        </div>

        <button class="build" disabled={!canBuild || status === "building"} onClick={build}>
          {status === "building" ? "Building…" : resultPath ? "Built ✓ — pick again to export anew" : "Build"}
        </button>

        {statusDetail && (
          <p class={`status ${status === "error" ? "bad" : status === "done" ? "ok" : ""}`}>
            {statusDetail}
            {status === "done" && resultPath && (
              <button class="reveal" onClick={() => revealItemInDir(resultPath)}>
                Show in Explorer
              </button>
            )}
          </p>
        )}

        {/* The hint occupies floor space only while there is no zip:
            once picked, the build status line takes its place so the
            card never outgrows the window (no scrolling). */}
        {!zipPath && (
          <p class="hint">
            Inspecting and window settings come with sensible defaults. The exported exe
            opens at 1280×720, resizable, with F11 fullscreen.
          </p>
        )}
      </section>

      <footer class="foot">
        <span>MIT · free &amp; open source</span>
        <span>v0.1.0</span>
      </footer>
    </main>
  );
}

export default App;
