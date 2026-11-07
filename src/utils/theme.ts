/** The active palette, single source of truth for every themed surface and the 3D preview. */

import type {Ref} from 'vue'
import {watchEffect} from 'vue'
import {useColorMode, useLocalStorage} from '@vueuse/core'

/** The two palettes declared in index.css */
export type ThemeName = 'dark' | 'light'

/** Stored palette choice; anything missing or unexpected falls back to dark. */
const stored = useLocalStorage<ThemeName>('assets-lens.theme', 'dark', {
    writeDefaults: false,
    serializer: {
        read: (raw) => (raw === 'light' ? 'light' : 'dark'),
        write: (value) => value,
    },
})

// Paints `data-theme` on <html>, where the color tokens are defined; `stored` keeps the existing key
useColorMode<ThemeName>({
    selector: 'html',
    attribute: 'data-theme',
    storageRef: stored,
    initialValue: 'dark',
    emitAuto: false,
})

// `useColorMode` paints the attribute only; `color-scheme` keeps native widgets in the same mode
watchEffect(() => {
    document.documentElement.style.colorScheme = stored.value
})

/** Reactive active theme; dark is the default */
export const theme: Ref<ThemeName> = stored

/** Flip and remember */
export function toggleTheme(): void {
    theme.value = theme.value === 'dark' ? 'light' : 'dark'
}
