/**
 * Web-side persistence for app settings: the game data directory, the language preference and the
 * palette choice all live in localStorage. The window rectangle is not here — the window-state
 * plugin owns it on the Rust side (see `lib.rs`).
 *
 * Storage is handled by VueUse's `useLocalStorage`: it reads once at module load, keeps the value
 * reactive and writes on change, including removing the key when the value is set to `null`. Each
 * entry keeps the validation the hand-written accessors used to do in its `read` serializer.
 *
 * The backend no longer writes settings.json — the frontend remembers the directory and hands it
 * back for validation on startup, so all setting reads/writes stay in the Web layer while the
 * backend only validates and uses it.
 */

import {useLocalStorage} from '@vueuse/core'

const GAME_PATH_KEY = 'assets-lens.game-path'
const LOCALE_KEY = 'assets-lens.locale'
const THEME_KEY = 'assets-lens.theme'

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
 * Stored language preference. `manual` marks an explicit user choice, which is kept forever;
 * without it the language keeps following the system on every launch.
 *
 * The locale is typed as a plain string here: the supported list lives in `i18n`, and keeping this
 * module free of that dependency avoids an import cycle.
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

/**
 * Stored palette choice. Dark is what the app has always shipped with, so anything missing,
 * unreadable or unexpected falls back to it rather than to the system preference.
 */
const themeSetting = useLocalStorage<'dark' | 'light'>(THEME_KEY, 'dark', {
    writeDefaults: false,
    serializer: {
        read: (raw) => (raw === 'light' ? 'light' : 'dark'),
        write: (value) => value,
    },
})

/** Read the remembered game data directory; returns null when localStorage is unavailable (private mode, etc.) */
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

/** Read the stored palette choice */
export function readThemeSetting(): 'dark' | 'light' {
    return themeSetting.value
}

/** Remember the palette choice */
export function writeThemeSetting(theme: 'dark' | 'light'): void {
    themeSetting.value = theme
}
