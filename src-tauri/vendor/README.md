# Vendored crates

## tauri-runtime-cef

Copied from https://github.com/tauri-apps/tauri
rev `f5c7530354cb66b5859b9f5d11c8ec1a053448b3` (`crates/tauri-runtime-cef`),
applied via `[patch."https://github.com/tauri-apps/tauri"]` in `../Cargo.toml`.

### Why

The custom window decorations (GNOME-style header bar, `cef-gnome-decorations`
branch) need the frameless window to be draggable. With `ozone-platform=wayland`
the CEF window is a native Wayland toplevel, so:

- Tauri's `startDragging()` cannot work: the runtime's Linux implementation
  sends X11 `_NET_WM_MOVERESIZE` using `window_handle()` as an XID, but under
  Wayland that handle is not an X window (crashes with `BadWindow` on id `0x1`
  when DISPLAY is set, silently no-ops otherwise).
- The only mechanism Chromium supports there is **draggable regions**
  (`app-region: drag` CSS): the renderer reports the regions, and the CEF Views
  window turns pointer-downs inside them into native titlebar drags
  (HTCAPTION → `xdg_toplevel.move` on Wayland — same path Chrome PWAs use).

Upstream implements region forwarding only on Windows (WndProc subclassing in
`src/cef_impl/drag_window.rs`); on Linux the renderer's regions were dropped.

### Local changes (vs upstream rev above)

All in `src/cef_impl.rs`, marked with `LOCAL PATCH` comments:

1. `BrowserDragHandler` now carries `window_id` + `context` (and an `enabled`
   flag for the original file drag-drop logic) and implements
   `on_draggable_regions_changed`, forwarding the regions to
   `cef::Window::set_draggable_regions`.
2. `BrowserClient::drag_handler()` always installs the handler (previously only
   when `dragDropEnabled` was true) so region forwarding works independently of
   file drag-drop; `on_drag_enter` gates itself to preserve prior behavior.

`Cargo.toml` was adjusted for standalone use: workspace-inherited keys replaced
with concrete values, `path` dependencies redirected to the same git rev.

### Removal

Delete this directory and the `[patch."https://github.com/tauri-apps/tauri"]`
section in `../Cargo.toml` once the patch (or equivalent) lands upstream.
