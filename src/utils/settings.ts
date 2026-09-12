/**
 * Web-side persistence for app settings: the game data directory and the language preference both
 * live in localStorage.
 *
 * The backend no longer writes settings.json — the frontend remembers the directory and hands it
 * back for validation on startup, so all setting reads/writes stay in the Web layer while the
 * backend only validates and uses it.
 */

const GAME_PATH_KEY = 'assets-lens.game-path'
const LOCALE_KEY = 'assets-lens.locale'

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
