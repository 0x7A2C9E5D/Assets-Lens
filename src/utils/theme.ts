/**
 * The active palette, as the single source of truth for every themed surface: the title bar toggle
 * writes it, the 3D preview reads it, and nothing else touches localStorage or the DOM for colour.
 */

import type {Ref} from 'vue'
import {ref} from 'vue'
import {readThemeSetting, writeThemeSetting} from './settings'

/** The two palettes declared in index.css */
export type ThemeName = 'dark' | 'light'

/** Reactive active theme; dark is the default, which is what the app has shipped with so far */
export const theme: Ref<ThemeName> = ref('dark')

/** Paint `next` on <html>: every colour token hangs off the `[data-theme]` selector */
export function applyTheme(next: ThemeName): void {
    const root = document.documentElement
    root.dataset.theme = next
    // Keeps native widgets (scrollbars, form controls) on the same side as the palette
    root.style.colorScheme = next
}

/** Read the stored choice before the first frame, so the window is revealed with the right palette */
export function initTheme(): void {
    theme.value = readThemeSetting()
    applyTheme(theme.value)
}

/** Flip and remember; a failed write only costs the memory for the next launch */
export function toggleTheme(): void {
    theme.value = theme.value === 'dark' ? 'light' : 'dark'
    applyTheme(theme.value)
    writeThemeSetting(theme.value)
}
