import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isLinux } from "./imagePreloader";

let cached: boolean | null = null;
let pending: Promise<boolean> | null = null;

function resolveCustomDecorations(): Promise<boolean> {
    if (!isLinux) return Promise.resolve(false);
    if (cached !== null) return Promise.resolve(cached);
    if (!pending) {
        pending = invoke<boolean>("should_use_custom_decorations")
            .then(v => { cached = v; return v; })
            .catch(() => { cached = false; return false; });
    }
    return pending;
}

// Whether the app should render its own GNOME-style header bar and resize
// handles instead of relying on the native window frame. Resolves to false
// on Windows/macOS and on tiling WMs, where the native/borderless frame is
// left untouched.
export function useCustomDecorations(): boolean {
    const [enabled, setEnabled] = useState<boolean>(cached ?? false);

    useEffect(() => {
        let cancelled = false;
        resolveCustomDecorations().then(v => { if (!cancelled) setEnabled(v); });
        return () => { cancelled = true; };
    }, []);

    return enabled;
}
