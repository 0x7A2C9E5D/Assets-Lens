/**
 * The active palette, as the single source of truth for every themed surface: the title bar toggle
 * writes it, the 3D preview reads it, and nothing else touches localStorage or the DOM for color.
 */

import type {Ref} from 'vue'
import {watchEffect} from 'vue'
import {useColorMode, useLocalStorage} from '@vueuse/core'

/** The two palettes declared in index.css */
export type ThemeName = 'dark' | 'light'

/**
 * Stored palette choice. Dark is what the app has always shipped with, so anything missing,
 * unreadable or unexpected falls back to it rather than to the system preference.
 */
const stored = useLocalStorage<ThemeName>('assets-lens.theme', 'dark', {
    writeDefaults: false,
    serializer: {
        read: (raw) => (raw === 'light' ? 'light' : 'dark'),
        write: (value) => value,
    },
})

// Paints `data-theme` on <html>, the selector every color token hangs off. `stored` is handed in as
// the store, so the storage key stays the one the app has always used and nothing is persisted twice
useColorMode<ThemeName>({
    selector: 'html',
    attribute: 'data-theme',
    storageRef: stored,
    initialValue: 'dark',
    emitAuto: false,
})

// `useColorMode` paints the attribute only; `color-scheme` keeps native widgets (scrollbars, form
// controls) on the same side as the palette
watchEffect(() => {
    document.documentElement.style.colorScheme = stored.value
})

/** Reactive active theme; dark is the default, which is what the app has shipped with so far */
export const theme: Ref<ThemeName> = stored

/** Flip and remember */
export function toggleTheme(): void {
    theme.value = theme.value === 'dark' ? 'light' : 'dark'
}
