import {Channel, invoke} from '@tauri-apps/api/core'
import {getCurrentWindow} from '@tauri-apps/api/window'
import {open} from '@tauri-apps/plugin-dialog'
import {openUrl} from '@tauri-apps/plugin-opener'

export interface BuildProgress {
    percent: number
}

export interface DatabaseStats {
    visualCount: number
    materialCount: number
    textureCount: number
    virtualTextureCount: number
}

export interface TextureRef {
    id: string
    name: string
    path: string
    width: number
    height: number
    parameterName: string | null
    /** Names of this asset's materials that bind this texture; absent when there are none to name.
     *  The material section inverts this relation (material name → the textures under it); a
     *  texture row itself carries no material reference */
    materialNames?: string[]
}

/** Material reference: the GUID stays the identity, the name is the readable label */
export interface MaterialRef {
    id: string
    name: string
    /** Base material template (`.lsf`) the material is derived from */
    sourceFile: string
}

export interface VirtualTextureRef {
    id: string
    name: string
    hash: string
    /** Page file (`.gtp`) inside its archive; empty when the hash resolved to no page file */
    path: string
    /** Pixel size of this page file, the same box the extractor writes as its DDS; null while its
     *  GTS could not be read */
    width: number | null
    height: number | null
    /** Names of this asset's materials that bind this virtual texture; absent when there are none
     *  to name. Inverted the same way as `TextureRef.materialNames`, so the material section can
     *  list a material's virtual textures alongside its regular ones */
    materialNames?: string[]
    /** Parameter the binding fills (e.g. `virtualtexture`, `overlayvirtualtexture`), read off the
     *  template of the material that binds it; absent when that template could not be read, so the
     *  chip renders without it */
    parameterName?: string
}

export interface VisualAsset {
    /** Visual resource ID (GUID) — the lookup key, since names are not unique */
    id: string
    name: string
    path: string
    materials: MaterialRef[]
    textures: TextureRef[]
    virtualTextures: VirtualTextureRef[]
}

export interface VisualSummary {
    /** Visual resource ID (GUID): the row identity — one name can belong to several visuals */
    id: string
    name: string
    materialCount: number
    textureCount: number
    virtualTextureCount: number
}

/** 3D preview: the GR2 mesh converted to GLB (transferred as Base64 to avoid per-byte JSON arrays) */
export interface ModelPreview {
    base64: string
}

export interface Page<T> {
    items: T[]
    total: number
    offset: number
}

/** App metadata shown on the About page */
export interface AppInfo {
    version: string
}

export function getAppInfo(): Promise<AppInfo> {
    return invoke<AppInfo>('app_info')
}

export function detectGamePath(): Promise<string | null> {
    return invoke<string | null>('detect_game_path')
}

export function getGamePath(): Promise<string | null> {
    return invoke<string | null>('get_game_path')
}

export function setGamePath(path: string): Promise<string> {
    return invoke<string>('set_game_path', {path})
}

export function buildDatabase(onProgress: Channel<BuildProgress>): Promise<DatabaseStats> {
    return invoke<DatabaseStats>('build_database', {onProgress})
}

export function dbStats(): Promise<DatabaseStats | null> {
    return invoke<DatabaseStats | null>('db_stats')
}

/** Sort columns the browse list offers; each maps to an order cached by the backend */
export type VisualSort = 'name' | 'id'

/**
 * One page of visual assets. `keyword` matches the asset name or its GUID; `sort` / `descending`
 * select the backend's cached order — sorting has to happen there, because the list is paged there.
 */
export function listVisuals(
    offset: number,
    limit: number,
    keyword?: string,
    sort: VisualSort = 'name',
    descending = false,
): Promise<Page<VisualSummary>> {
    return invoke<Page<VisualSummary>>('list_visuals', {
        offset,
        limit,
        keyword: keyword && keyword.trim() ? keyword : null,
        sort,
        descending,
    })
}

export function getVisual(id: string): Promise<VisualAsset | null> {
    return invoke<VisualAsset | null>('get_visual', {id})
}

/** Fetch the GLB mesh geometry (no textures) converted from the asset's GR2 file for the three.js preview */
export function getVisualPreview(path: string): Promise<ModelPreview> {
    return invoke<ModelPreview>('get_visual_preview', {path})
}

/* ---------- Asset export ---------- */

/** Mesh export format: raw GR2 out of the archive, or a converted GLB */
export type MeshFormat = 'gr2' | 'glb'

/** Texture export format for separate files */
export type TextureFormat = 'none' | 'dds' | 'png'

export interface ExportOptions {
    /** Mesh output format: raw GR2 or converted GLB (defaults to GR2) */
    meshFormat: MeshFormat
    /** Texture output format for separate files — covers both regular textures and virtual textures */
    textureFormat: TextureFormat
}

/** Export progress; phase is one of prepare / model / modelRaw / textures / virtualTextures / manifest / done */
export interface ExportProgress {
    phase: string
    currentFile: string | null
    percent: number
}

export interface ExportedFile {
    path: string
    /** Artifact kind: gr2 / glb / dds / png */
    kind: string
    sizeBytes: number
}

/**
 * Export warning: `code` is a stable enum (mapped to i18n copy on the frontend) while `detail`
 * holds the raw detail; for unknown codes (e.g. maclarian passthroughs) the detail is shown as-is
 */
export interface ExportWarning {
    code: string
    detail: string
}

export interface ExportResult {
    /** The directory actually written to (selected directory/<asset name>) */
    outputDir: string
    files: ExportedFile[]
    warnings: ExportWarning[]
}

/**
 * Export a single visual asset (addressed by its GUID — the name is not unique) to the given
 * directory; the backend creates a subdirectory named after the asset. Progress is pushed stage by
 * stage through the Channel.
 */
export function exportVisualAsset(
    id: string,
    destDir: string,
    options: ExportOptions,
    onProgress: Channel<ExportProgress>,
): Promise<ExportResult> {
    return invoke<ExportResult>('export_visual_asset', {id, destDir, options, onProgress})
}

/** Open the system directory picker and return the selected path (null when canceled) */
export async function pickDirectory(defaultPath?: string): Promise<string | null> {
    const selected = await open({
        directory: true,
        multiple: false,
        defaultPath,
    })
    return typeof selected === 'string' ? selected : null
}

/* ---------- External links ---------- */

/** Open an external URL in the system's default browser (used by the project links on the About page) */
export function openExternal(url: string): Promise<void> {
    return openUrl(url)
}

/* ---------- Custom window controls (frameless mode) ---------- */

/**
 * Start dragging the window when the title bar is pressed (a frameless window needs this explicit
 * call; the data-tauri-drag-region attribute is not reliable)
 */
export function startWindowDrag(): Promise<void> {
    return getCurrentWindow().startDragging()
}

export function minimizeWindow(): Promise<void> {
    return getCurrentWindow().minimize()
}

export function toggleMaximizeWindow(): Promise<void> {
    return getCurrentWindow().toggleMaximize()
}

export function closeWindow(): Promise<void> {
    return getCurrentWindow().close()
}

export function isWindowMaximized(): Promise<boolean> {
    return getCurrentWindow().isMaximized()
}

/** Subscribe to window resize events (returns the unsubscribe function) */
export function onWindowResized(handler: () => void): Promise<() => void> {
    return getCurrentWindow().onResized(handler)
}
