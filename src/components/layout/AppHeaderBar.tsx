import React, { useEffect, useRef, useState } from "react";
import { Minus, Copy, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCustomDecorations } from "../../utils/useCustomDecorations";

const HEADER_HEIGHT = 38;

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

        return () => {
            document.documentElement.classList.remove("ttl-csd");
            unlistenPromise.then(unlisten => unlisten());
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
            className="ttl-drag-region fixed top-0 left-0 right-0 z-[100] flex items-center justify-between select-none animate-fadeIn"
            style={{
                height: HEADER_HEIGHT,
                background: "#1c1526",
                borderBottom: "1px solid rgba(192,132,252,0.15)",
            }}
            onMouseDown={onBarMouseDown}
        >
            <div className="flex items-center gap-2 pl-3 pointer-events-none">
                <img src="/launcher-icon.png" alt="" className="w-4 h-4" />
                <span className="text-sm font-medium text-purple-100">Twintail Launcher</span>
            </div>
            <div className="ttl-no-drag flex items-center h-full">
                <button
                    className="h-full w-11 flex items-center justify-center text-purple-200/70 hover:bg-purple-600/30 transition-colors"
                    onMouseDown={(e) => e.stopPropagation()}
                    onClick={() => getCurrentWindow().minimize().catch(() => {})}
                    aria-label="Minimize"
                >
                    <Minus size={15} />
                </button>
                <button
                    className="h-full w-11 flex items-center justify-center text-purple-200/70 hover:bg-purple-600/30 transition-colors"
                    onMouseDown={(e) => e.stopPropagation()}
                    onClick={() => getCurrentWindow().toggleMaximize().catch(() => {})}
                    aria-label={isMaximized ? "Restore" : "Maximize"}
                >
                    {isMaximized ? <Copy size={13} /> : <Square size={12} />}
                </button>
                <button
                    className="h-full w-11 flex items-center justify-center text-purple-200/70 hover:bg-red-500/80 hover:text-white transition-colors"
                    onMouseDown={(e) => e.stopPropagation()}
                    onClick={() => getCurrentWindow().close().catch(() => {})}
                    aria-label="Close"
                >
                    <X size={15} />
                </button>
            </div>
        </div>
    );
};

export default AppHeaderBar;
