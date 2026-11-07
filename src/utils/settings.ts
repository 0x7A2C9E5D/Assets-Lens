/**
 * Web-side persistence for app settings: the game data directory and the language preference live
 * in localStorage. The window rectangle is owned by the window-state plugin and the palette by
 * `theme.ts`.
 */

import {useLocalStorage} from '@vueuse/core'

const GAME_PATH_KEY = 'assets-lens.game-path'
const LOCALE_KEY = 'assets-lens.locale'

/** Read the remembered game data directory; null when unset or blank */
const gamePath = useLocalStorage<string | null>(GAME_PATH_KEY, null, {
    // The first launch must not write a default into storage
    writeDefaults: false,
    serializer: {
        read: (raw) => (raw.trim() ? raw : null),
        write: (value) => value ?? '',
    },
})

/**
 * Stored language preference; `manual` marks an explicit user choice, kept forever. Typed as a
 * plain string to keep this module free of the `i18n` import cycle.
 */
export interface LocaleSetting {
    locale: string
    manual?: boolean
}

/** Read the stored language preference; null when unset, unreadable or corrupted */
const localeSetting = useLocalStorage<LocaleSetting | null>(LOCALE_KEY, null, {
    writeDefaults: false,
    serializer: {
        read: (raw) => {
            try {
                const parsed: unknown = JSON.parse(raw)
                return parsed &&
                    typeof parsed === 'object' &&
                    typeof (parsed as LocaleSetting).locale === 'string'
                    ? (parsed as LocaleSetting)
                    : null
            } catch {
                // Corrupted data: fall back to the system language
                return null
            }
        },
        write: (value) => JSON.stringify(value satisfies LocaleSetting | null),
    },
})

/** Read the remembered game data directory; null when storage is unavailable */
export function readGamePath(): string | null {
    return gamePath.value
}

/** Remember the current game data directory */
export function writeGamePath(path: string): void {
    gamePath.value = path
}

/** Drop the record when the directory no longer exists, so the next launch does not retry it */
export function clearGamePath(): void {
    gamePath.value = null
}

/** Read the stored language preference; returns null when unset, unreadable or corrupted */
export function readLocaleSetting(): LocaleSetting | null {
    return localeSetting.value
}

/** Remember the language preference */
export function writeLocaleSetting(locale: string, manual: boolean): void {
    localeSetting.value = {locale, manual}
}
