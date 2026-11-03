import { useEffect, useState } from "preact/hooks";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
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
  const [status, setStatus] = useState("idle"); // idle | building | done | error
  const [statusDetail, setStatusDetail] = useState("");

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem(THEME_KEY, theme);
  }, [theme]);

  async function pickZip() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "HTML5 build (zip)", extensions: ["zip"] }],
    });
    if (!picked) return;
    setZipPath(picked);
    setZipInfo(null);
    setInspectError("");
    setTitle(prettyName(picked));
    try {
      const info = await invoke("inspect_zip", { path: picked });
      setZipInfo(info);
    } catch (error) {
      setInspectError(String(error));
    }
  }

  async function pickIcon() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "Windows icon", extensions: ["ico"] }],
    });
    if (picked) setIconPath(picked);
  }

  const canBuild = Boolean(zipPath) && Boolean(zipInfo?.entry) && title.trim().length > 0;

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
              <span class="dropzone-big">Click to choose a .zip</span>
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
            <span class="field-label">Icon (.ico, optional)</span>
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

        <button class="build" disabled={!canBuild || status === "building"}>
          {status === "building" ? "Building…" : "Build"}
        </button>

        {statusDetail && (
          <p class={`status ${status === "error" ? "bad" : status === "done" ? "ok" : ""}`}>
            {statusDetail}
          </p>
        )}

        <p class="hint">
          Inspecting and window settings come with sensible defaults. The exported exe
          opens at 1280×720, resizable, with F11 fullscreen.
        </p>
      </section>

      <footer class="foot">
        <span>MIT · free &amp; open source</span>
        <span>v0.1.0</span>
      </footer>
    </main>
  );
}

export default App;
