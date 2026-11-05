/**
 * Web-side persistence for app settings: the game data directory, the language preference and the
 * window rectangle all live in localStorage.
 *
 * The backend no longer writes settings.json — the frontend remembers the directory and hands it
 * back for validation on startup, so all setting reads/writes stay in the Web layer while the
 * backend only validates and uses it.
 */

const GAME_PATH_KEY = 'assets-lens.game-path'
const LOCALE_KEY = 'assets-lens.locale'
const THEME_KEY = 'assets-lens.theme'
const WINDOW_KEY = 'assets-lens.window'

/** Read the remembered game data directory; returns null when localStorage is unavailable (private mode, etc.) */
export function readGamePath(): string | null {
    try {
        const raw = localStorage.getItem(GAME_PATH_KEY)
        return raw && raw.trim() ? raw : null
    } catch {
        return null
    }
}

/** Remember the current game data directory */
export function writeGamePath(path: string): void {
    try {
        localStorage.setItem(GAME_PATH_KEY, path)
    } catch {
        // A failed write only costs the memory for the next launch; this session is unaffected
    }
}

/** Drop the record when the directory no longer exists, so the next launch does not retry it */
export function clearGamePath(): void {
    try {
        localStorage.removeItem(GAME_PATH_KEY)
    } catch {
        // Same as above: safe to ignore
    }
}

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

/** Read the stored language preference; returns null when unset, unreadable or corrupted */
export function readLocaleSetting(): LocaleSetting | null {
    try {
        const raw = localStorage.getItem(LOCALE_KEY)
        if (!raw) return null
        const parsed: unknown = JSON.parse(raw)
        if (
            parsed &&
            typeof parsed === 'object' &&
            typeof (parsed as LocaleSetting).locale === 'string'
        ) {
            return parsed as LocaleSetting
        }
        return null
    } catch {
        // Unavailable (private mode) or corrupted data: fall back to the system language
        return null
    }
}

/** Remember the language preference */
export function writeLocaleSetting(locale: string, manual: boolean): void {
    try {
        localStorage.setItem(LOCALE_KEY, JSON.stringify({locale, manual} satisfies LocaleSetting))
    } catch {
        // A failed write only costs the memory for the next launch; this session is unaffected
    }
}

/**
 * The window rectangle to come back to, in physical pixels, plus whether the window was left
 * maximized. Only the normal rectangle is ever stored: a maximized window reports the work area,
 * which is not the size it returns to when un-maximized.
 */
export interface WindowGeometry {
    x: number
    y: number
    width: number
    height: number
    maximized: boolean
}

/** Read the remembered window rectangle; returns null when unset, unreadable or malformed */
export function readWindowGeometry(): WindowGeometry | null {
    try {
        const raw = localStorage.getItem(WINDOW_KEY)
        if (!raw) return null
        const parsed: unknown = JSON.parse(raw)
        if (!parsed || typeof parsed !== 'object') return null
        const {x, y, width, height, maximized} = parsed as WindowGeometry
        const values = [x, y, width, height]
        if (!values.every((value) => typeof value === 'number' && Number.isFinite(value))) return null
        // A zero-sized or inverted rectangle would be applied as a window nobody can use
        if (width <= 0 || height <= 0) return null
        return {x, y, width, height, maximized: maximized}
    } catch {
        // Unavailable (private mode) or corrupted data: the window keeps its configured rectangle
        return null
    }
}

/** Remember the window rectangle */
export function writeWindowGeometry(geometry: WindowGeometry): void {
    try {
        localStorage.setItem(WINDOW_KEY, JSON.stringify(geometry))
    } catch {
        // A failed write only costs the memory for the next launch; this session is unaffected
    }
}

/**
 * Stored palette choice. Dark is what the app has always shipped with, so anything missing,
 * unreadable or unexpected falls back to it rather than to the system preference.
 */
export function readThemeSetting(): 'dark' | 'light' {
    try {
        return localStorage.getItem(THEME_KEY) === 'light' ? 'light' : 'dark'
    } catch {
        return 'dark'
    }
}

/** Remember the palette choice */
export function writeThemeSetting(theme: 'dark' | 'light'): void {
    try {
        localStorage.setItem(THEME_KEY, theme)
    } catch {
        // A failed write only costs the memory for the next launch; this session is unaffected
    }
}
