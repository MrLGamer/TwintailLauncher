import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SettingsSection, ModernToggle, ModernInput, ModernPathInput, ModernSelect } from "../common/SettingsComponents.tsx";
import { translate } from "../../utils/i18n";

// Structured editor for the PROTON_* options understood by the Anime-Proton runner.
// Everything is stored in the install's existing env_vars string (the same one the
// free-text field on the Launch tab edits); entries this section does not manage are
// passed through untouched.

interface ProtonAnimeSettingsProps {
    installSettings: any;
    gameBiz: string;
    handleUpdate: (key: string, value: any) => void;
}

const IMPORTER_VARS: Record<string, string> = {
    xxmi: "PROTON_USE_XXMI",
    gimi: "PROTON_USE_GIMI",
    srmi: "PROTON_USE_SRMI",
    zzmi: "PROTON_USE_ZZMI",
    himi: "PROTON_USE_HIMI",
    wwmi: "PROTON_USE_WWMI",
    efmi: "PROTON_USE_EFMI",
};

// biz prefix -> the importer matching that game
const BIZ_IMPORTER: [string, string][] = [
    ["hk4e", "gimi"],
    ["hkrpg", "srmi"],
    ["nap", "zzmi"],
    ["bh3", "himi"],
    ["wuwa", "wwmi"],
    ["endfield", "efmi"],
];

// Semicolons inside double quotes are part of the value, not separators
function splitEnvVars(raw: string): string[] {
    const parts: string[] = [];
    let current = "";
    let inQuotes = false;
    for (const ch of raw) {
        if (ch === '"') { inQuotes = !inQuotes; current += ch; }
        else if (ch === ";" && !inQuotes) { parts.push(current); current = ""; }
        else current += ch;
    }
    parts.push(current);
    return parts.filter(p => p.trim() !== "");
}

function parseEnvVars(raw: string): { key: string, value: string }[] {
    return splitEnvVars(raw || "").map(entry => {
        const eq = entry.indexOf("=");
        if (eq === -1) return { key: entry.trim(), value: "" };
        return { key: entry.slice(0, eq).trim(), value: entry.slice(eq + 1).replace(/"/g, "") };
    });
}

function serializeEnvVars(entries: { key: string, value: string }[]): string {
    return entries.map(e => e.value.includes(";") ? `${e.key}="${e.value}"` : `${e.key}=${e.value}`).join(";");
}

const WarningNote = ({ text }: { text: string }) => (
    <div className="rounded-xl border border-yellow-500/30 bg-yellow-950/20 p-4 text-sm text-yellow-200">
        {text}
    </div>
);

export default function ProtonAnimeSettings({ installSettings, gameBiz, handleUpdate }: ProtonAnimeSettingsProps) {
    // The launcher's global XXMI location, used for the "use TTL mods folder" toggle
    const [ttlXxmiPath, setTtlXxmiPath] = useState("");
    useEffect(() => {
        invoke<any | null>("list_settings")
            .then(gs => setTtlXxmiPath(gs?.xxmi_path || ""))
            .catch(() => setTtlXxmiPath(""));
    }, []);

    const entries = parseEnvVars(installSettings.env_vars || "");
    const getVar = (key: string) => entries.find(e => e.key === key)?.value;
    const isOn = (key: string) => {
        const v = getVar(key);
        return v !== undefined && v !== "" && v !== "0";
    };

    // null removes a variable, any string sets it; unlisted entries are kept as-is
    const setVars = (changes: Record<string, string | null>) => {
        const next = entries.map(e => ({ ...e }));
        for (const [key, value] of Object.entries(changes)) {
            const idx = next.findIndex(e => e.key === key);
            if (value === null) {
                if (idx !== -1) next.splice(idx, 1);
            } else if (idx !== -1) {
                next[idx].value = value;
            } else {
                next.push({ key, value });
            }
        }
        handleUpdate("env_vars", serializeEnvVars(next));
    };

    const activeImporter = Object.keys(IMPORTER_VARS).find(k => isOn(IMPORTER_VARS[k])) || "off";
    const gameImporter = BIZ_IMPORTER.find(([biz]) => gameBiz.startsWith(biz))?.[1];
    const importerOptions = [
        { value: "off", name: translate("game_settings.proton_anime.importer.off") },
        { value: "xxmi", name: translate("game_settings.proton_anime.importer.auto") },
        ...(gameImporter
            ? [{ value: gameImporter, name: IMPORTER_VARS[gameImporter].replace("PROTON_USE_", "") }]
            : Object.keys(IMPORTER_VARS).filter(k => k !== "xxmi").map(k => ({ value: k, name: k.toUpperCase() }))),
    ];
    // A manually typed importer var not offered above still shows correctly
    if (activeImporter !== "off" && !importerOptions.some(o => o.value === activeImporter)) {
        importerOptions.push({ value: activeImporter, name: activeImporter.toUpperCase() });
    }

    const setImporter = (val: string) => {
        const changes: Record<string, string | null> = {};
        for (const [key, envVar] of Object.entries(IMPORTER_VARS)) {
            changes[envVar] = key === val ? "1" : null;
        }
        setVars(changes);
    };

    // The mods folder TTL's own XXMI deployment uses for this game; the importer is
    // known either from an explicit selection or from the game when auto-detecting
    const effectiveImporter = activeImporter === "xxmi" ? gameImporter : (activeImporter !== "off" ? activeImporter : undefined);
    const ttlModsPath = ttlXxmiPath && effectiveImporter ? `${ttlXxmiPath.replace(/\/+$/, "")}/${effectiveImporter}/Mods` : "";
    const usingTtlMods = !!ttlModsPath && getVar("PROTON_XXMI_MODS_PATH") === ttlModsPath;

    const optiscalerOn = isOn("PROTON_USE_OPTISCALER");
    const reshadeOn = isOn("PROTON_USE_RESHADE");
    const showGiFps = gameBiz.startsWith("hk4e");
    const showHsrFps = gameBiz.startsWith("hkrpg");
    const showWuwaVars = gameBiz.startsWith("wuwa");
    const protonFpsActive = (showGiFps && !!getVar("PROTON_GI_FPS")) || (showHsrFps && !!getVar("PROTON_HSR_FPS"));

    return (
        <>
            <SettingsSection title={translate("game_settings.proton_anime.title")}>
                <div className="rounded-xl border border-white/10 bg-zinc-900/70 p-4 text-sm text-zinc-300">
                    {translate("game_settings.proton_anime.note")}
                </div>
                <ModernSelect
                    label={translate("game_settings.proton_anime.importer.label")}
                    description={translate("game_settings.proton_anime.importer.description")}
                    value={activeImporter}
                    options={importerOptions}
                    onChange={setImporter}
                />
                {activeImporter !== "off" && installSettings.use_xxmi && (
                    <WarningNote text={translate("game_settings.proton_anime.warning.xxmi_conflict")} />
                )}
                {activeImporter !== "off" && (
                    <>
                        <ModernToggle
                            label={translate("game_settings.proton_anime.ttl_mods.label")}
                            description={translate("game_settings.proton_anime.ttl_mods.description")}
                            disabled={!ttlModsPath}
                            checked={usingTtlMods}
                            onChange={(val) => setVars({ PROTON_XXMI_MODS_PATH: val ? ttlModsPath : null })}
                        />
                        <ModernPathInput
                            label={translate("game_settings.proton_anime.mods_path.label")}
                            description={translate("game_settings.proton_anime.mods_path.description")}
                            value={getVar("PROTON_XXMI_MODS_PATH") || ""}
                            onChange={(val) => setVars({ PROTON_XXMI_MODS_PATH: val || null })}
                        />
                    </>
                )}
                <ModernToggle
                    label={translate("game_settings.proton_anime.optiscaler.label")}
                    description={translate("game_settings.proton_anime.optiscaler.description")}
                    checked={optiscalerOn}
                    onChange={(val) => setVars({ PROTON_USE_OPTISCALER: val ? "1" : null })}
                />
                {optiscalerOn && (
                    <>
                        <ModernInput
                            label={translate("game_settings.proton_anime.optiscaler_name.label")}
                            description={translate("game_settings.proton_anime.optiscaler_name.description")}
                            value={getVar("PROTON_OPTISCALER_NAME") || ""}
                            placeholder="dxgi.dll"
                            onChange={(e) => setVars({ PROTON_OPTISCALER_NAME: e.target.value || null })}
                        />
                        <ModernInput
                            label={translate("game_settings.proton_anime.optiscaler_config.label")}
                            description={translate("game_settings.proton_anime.optiscaler_config.description")}
                            value={getVar("PROTON_OPTISCALER_CONFIG") || ""}
                            placeholder="Upscalers.Dx11Upscaler=fsr31;Upscalers.Dx12Upscaler=dlss"
                            onChange={(e) => setVars({ PROTON_OPTISCALER_CONFIG: e.target.value || null })}
                        />
                    </>
                )}
                <ModernToggle
                    label={translate("game_settings.proton_anime.reshade.label")}
                    description={translate("game_settings.proton_anime.reshade.description")}
                    checked={reshadeOn}
                    onChange={(val) => setVars({ PROTON_USE_RESHADE: val ? "1" : null })}
                />
                {reshadeOn && optiscalerOn && activeImporter === "off" && (
                    <WarningNote text={translate("game_settings.proton_anime.warning.reshade_optiscaler")} />
                )}
                {reshadeOn && (
                    <>
                        <ModernToggle
                            label={translate("game_settings.proton_anime.reshade_addon.label")}
                            description={translate("game_settings.proton_anime.reshade_addon.description")}
                            checked={isOn("PROTON_RESHADE_ADDON")}
                            onChange={(val) => setVars({ PROTON_RESHADE_ADDON: val ? "1" : null })}
                        />
                        <ModernPathInput
                            label={translate("game_settings.proton_anime.reshade_path.label")}
                            description={translate("game_settings.proton_anime.reshade_path.description")}
                            value={getVar("PROTON_RESHADE_PATH") || ""}
                            onChange={(val) => setVars({ PROTON_RESHADE_PATH: val || null })}
                        />
                    </>
                )}
            </SettingsSection>

            {(showGiFps || showHsrFps || showWuwaVars) && (
                <SettingsSection title={translate("game_settings.proton_anime.fixes.title")}>
                    {protonFpsActive && installSettings.use_fps_unlock && (
                        <WarningNote text={translate("game_settings.proton_anime.warning.fps_conflict")} />
                    )}
                    {showGiFps && (
                        <ModernInput
                            type="number"
                            label={translate("game_settings.proton_anime.gi_fps.label")}
                            description={translate("game_settings.proton_anime.gi_fps.description")}
                            value={getVar("PROTON_GI_FPS") || ""}
                            placeholder="120"
                            onChange={(e) => setVars({ PROTON_GI_FPS: e.target.value || null })}
                        />
                    )}
                    {showHsrFps && (
                        <ModernInput
                            type="number"
                            label={translate("game_settings.proton_anime.hsr_fps.label")}
                            description={translate("game_settings.proton_anime.hsr_fps.description")}
                            value={getVar("PROTON_HSR_FPS") || ""}
                            placeholder="120"
                            onChange={(e) => setVars({ PROTON_HSR_FPS: e.target.value || null })}
                        />
                    )}
                    {showWuwaVars && (
                        <ModernInput
                            label={translate("game_settings.proton_anime.wuwa_engine_vars.label")}
                            description={translate("game_settings.proton_anime.wuwa_engine_vars.description")}
                            value={getVar("PROTON_WUWA_ENGINE_VARS") || ""}
                            placeholder="r.Streaming.HiddenPrimitiveScale=25,r.Kuro.SkeletalMesh.DistanceLODBaseFOV=165"
                            onChange={(e) => setVars({ PROTON_WUWA_ENGINE_VARS: e.target.value || null })}
                        />
                    )}
                </SettingsSection>
            )}
        </>
    );
}
