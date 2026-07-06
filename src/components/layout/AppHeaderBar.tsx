import React, { useEffect, useRef, useState } from "react";
import { Minus, Copy, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCustomDecorations } from "../../utils/useCustomDecorations";

const HEADER_HEIGHT = 46;

const AppHeaderBar: React.FC = () => {
    const enabled = useCustomDecorations();
    const [isMaximized, setIsMaximized] = useState(false);
    const resizeTimeout = useRef<number | undefined>(undefined);

    useEffect(() => {
        if (!enabled) return;

        document.documentElement.classList.add("ttl-csd");
        const win = getCurrentWindow();

        win.isMaximized().then(setIsMaximized).catch(() => {});
        const unlistenPromise = win.onResized(() => {
            window.clearTimeout(resizeTimeout.current);
            resizeTimeout.current = window.setTimeout(() => {
                win.isMaximized().then(setIsMaximized).catch(() => {});
            }, 50);
        });

        // System light/dark style, via the settings portal on the backend —
        // CEF never updates prefers-color-scheme on Linux, so the media query
        // can't be used for this.
        const applyTheme = (theme: string) => document.documentElement.classList.toggle("ttl-light", theme === "light");
        invoke<string>("get_system_theme").then(applyTheme).catch(() => {});
        const unlistenThemePromise = listen<string>("system_theme_changed", e => applyTheme(e.payload));

        return () => {
            document.documentElement.classList.remove("ttl-csd", "ttl-light");
            unlistenPromise.then(unlisten => unlisten());
            unlistenThemePromise.then(unlisten => unlisten());
            window.clearTimeout(resizeTimeout.current);
        };
    }, [enabled]);

    if (!enabled) return null;

    // Dragging is handled natively by Chromium via the `app-region: drag` CSS on the
    // bar (.ttl-drag-region) — the window is a Wayland-native CEF toplevel, so the
    // Tauri startDragging() X11 path must not be used (invalid X window handle).
    // Double-click fallback: when the native drag region is active the DOM never sees
    // these clicks, so this only fires if the region isn't handled natively.
    const onBarMouseDown = (e: React.MouseEvent) => {
        if (e.button !== 0) return;
        if (e.detail === 2) { getCurrentWindow().toggleMaximize().catch(() => {}); }
    };

    return (
        <div
            className="ttl-drag-region fixed top-0 left-0 right-0 z-[100] flex items-center justify-end select-none animate-fadeIn"
            style={{
                height: HEADER_HEIGHT,
                background: "var(--ttl-headerbar-bg)",
                boxShadow: "var(--ttl-headerbar-shadow)",
            }}
            onMouseDown={onBarMouseDown}
        >
            <span className="absolute left-1/2 -translate-x-1/2 text-[0.85rem] font-semibold tracking-wide pointer-events-none" style={{ color: "var(--ttl-headerbar-fg)" }}>Twintail Launcher</span>
            <div className="ttl-no-drag flex items-center gap-3 pr-3">
                <button
                    className="ttl-winctl"
                    onMouseDown={(e) => e.stopPropagation()}
                    onClick={() => getCurrentWindow().minimize().catch(() => {})}
                    aria-label="Minimize"
                >
                    <Minus size={12} strokeWidth={2.25} />
                </button>
                <button
                    className="ttl-winctl"
                    onMouseDown={(e) => e.stopPropagation()}
                    onClick={() => getCurrentWindow().toggleMaximize().catch(() => {})}
                    aria-label={isMaximized ? "Restore" : "Maximize"}
                >
                    {isMaximized ? <Copy size={10} strokeWidth={2.25} /> : <Square size={9} strokeWidth={2.5} />}
                </button>
                <button
                    className="ttl-winctl"
                    onMouseDown={(e) => e.stopPropagation()}
                    onClick={() => getCurrentWindow().close().catch(() => {})}
                    aria-label="Close"
                >
                    <X size={12} strokeWidth={2.25} />
                </button>
            </div>
        </div>
    );
};

export default AppHeaderBar;
