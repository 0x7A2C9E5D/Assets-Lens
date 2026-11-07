import {Channel} from '@tauri-apps/api/core'
import {getCurrentWindow} from '@tauri-apps/api/window'
import {open} from '@tauri-apps/plugin-dialog'
import {openUrl} from '@tauri-apps/plugin-opener'

import type {
    AppInfo,
    BuildProgress,
    DatabaseStats,
    ExportOptions_Deserialize,
    ExportProgress_Deserialize,
    ExportResult,
    MaterialSummary,
    ModelPreview,
    Page,
    TextureSummary_Serialize,
    VirtualTextureSummary_Serialize,
    VisualAssetDetail_Serialize,
    VisualSummary,
} from './bindings'
import {commands} from './bindings'

/* Types generated from the Rust models; the views keep importing them under these names */
export type {
    AppInfo,
    BuildProgress,
    DatabaseStats,
    ExportResult,
    ExportedFile,
    ExportWarning,
    MeshFormat,
    ModelPreview,
    Page,
    TextureFormat,
    VisualSummary,
} from './bindings'

export type VisualAsset = VisualAssetDetail_Serialize
export type TextureRef = TextureSummary_Serialize
export type MaterialRef = MaterialSummary
export type VirtualTextureRef = VirtualTextureSummary_Serialize
export type ExportOptions = ExportOptions_Deserialize
export type ExportProgress = ExportProgress_Deserialize

export function getAppInfo(): Promise<AppInfo> {
    return commands.appInfo()
}

export function detectGamePath(): Promise<string | null> {
    return commands.detectGamePath()
}

export function getGamePath(): Promise<string | null> {
    return commands.getGamePath()
}

export function setGamePath(path: string): Promise<string> {
    return commands.setGamePath(path)
}

export function buildDatabase(onProgress: Channel<BuildProgress>): Promise<DatabaseStats> {
    return commands.buildDatabase(onProgress)
}

export function dbStats(): Promise<DatabaseStats | null> {
    return commands.dbStats()
}

/** Sort columns the browse list offers; each maps to an order cached by the backend */
export type VisualSort = 'name' | 'id'

/** One page of visual assets; sorting happens in the backend, where the list is paged. */
export function listVisuals(
    offset: number,
    limit: number,
    keyword?: string,
    sort: VisualSort = 'name',
    descending = false,
): Promise<Page<VisualSummary>> {
    return commands.listVisuals(offset, limit, keyword && keyword.trim() ? keyword : null, sort, descending)
}

export function getVisual(id: string): Promise<VisualAsset | null> {
    return commands.getVisual(id)
}

/** Fetch the GLB mesh geometry (no textures) for the three.js preview */
export function getVisualPreview(path: string): Promise<ModelPreview> {
    return commands.getVisualPreview(path)
}

/**
 * Export a visual asset (by GUID) into a subdirectory of `destDir`; progress is pushed through the
 * Channel.
 */
export function exportVisualAsset(
    id: string,
    destDir: string,
    options: ExportOptions,
    onProgress: Channel<ExportProgress>,
): Promise<ExportResult> {
    return commands.exportVisualAsset(id, destDir, options, onProgress)
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

/** Open an external URL in the system browser (used by the About page links) */
export function openExternal(url: string): Promise<void> {
    return openUrl(url)
}

/**
 * Start dragging the window from the title bar (`data-tauri-drag-region` is not reliable).
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
