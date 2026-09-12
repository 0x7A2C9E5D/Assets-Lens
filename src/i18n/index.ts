import {watch} from 'vue'
import {createI18n} from 'vue-i18n'
import {getCurrentWindow} from '@tauri-apps/api/window'
import {readLocaleSetting, writeLocaleSetting} from '../utils/settings'
import enUS, {type LocaleMessages} from './locales/en-US'
import zhCN from './locales/zh-CN'
import zhTW from './locales/zh-TW'

/* ---------- Culture matching ---------- */
// Matching leans on `Intl` and its CLDR likely subtags (`zh` → `zh-Hans-CN`) instead of handwritten
// tag parsing, so a shipped language needs nothing but its bundle and one entry in `BUNDLES`.

/** Parsed locale used by the matcher */
interface LocaleInfo {
    /** Tag with likely subtags filled in, e.g. `zh-Hans-CN` */
    maximized: string
    /** Language plus script, region dropped, e.g. `zh-Hans` */
    neutral: string
    /** Bare language subtag, e.g. `zh` */
    language: string
}

/**
 * Parse an OS/browser locale tag. A malformed tag degrades to the cleaned string instead of
 * throwing, so callers never have to guard the result.
 */
function parseCulture(raw: string): LocaleInfo {
    // Clean up first: Intl rejects POSIX-style tags such as `zh_CN.UTF-8` outright
    const cleaned = raw.trim().toLowerCase().replace(/_/g, '-').split(/[.@]/)[0]
    try {
        const maximized = new Intl.Locale(cleaned).maximize()
        return {
            maximized: maximized.baseName.toLowerCase(),
            neutral: (maximized.script
                ? `${maximized.language}-${maximized.script}`
                : maximized.language
            ).toLowerCase(),
            language: maximized.language.toLowerCase(),
        }
    } catch {
        // Malformed tag, or an engine without Intl.Locale
        const language = cleaned.split('-')[0]
        return {maximized: cleaned, neutral: language, language}
    }
}

/**
 * Every shipped UI language: its bundle, under the code it answers to. Bundles are listed rather than
 * discovered, so the compiler and the IDE see each one being used.
 */
const BUNDLES = {
    'en-US': enUS,
    'zh-CN': zhCN,
    'zh-TW': zhTW,
} satisfies Record<string, LocaleMessages>

/** The shipped codes as a real union, so an unknown code is a compile error */
export type LocaleCode = keyof typeof BUNDLES

/** Shipped codes; the labels need the whole set, because their wording depends on it */
const CODES: readonly LocaleCode[] = (Object.keys(BUNDLES) as LocaleCode[]).sort((a, b) =>
    a.localeCompare(b),
)

/** The only bundle for its language, so naming it after the language alone cannot be ambiguous */
function isLoneBundleForLanguage(code: LocaleCode): boolean {
    const language = parseCulture(code).language
    return CODES.filter((other) => parseCulture(other).language === language).length === 1
}

/**
 * Names pinned by hand for the codes whose conventional wording we want rather than whatever the
 * engine phrases. `Intl` cannot be trusted here: its wording comes from the runtime's CLDR data, and
 * Node and WebView2 disagree — `of('zh-Hans')` is `简体中文` on Node 24 but `中文（简体）` in the
 * WebView (which is also why the region form reads `中文（中国）`).
 */
const LANGUAGE_LABELS: Partial<Record<LocaleCode, string>> = {
    'zh-CN': '简体中文',
    'zh-TW': '繁體中文',
}

/**
 * A language's own name, from `Intl` (`zh-CN` → 中文) rather than a hand-kept label table. The bare
 * language is preferred because the regional form is wordy (`en-US` → American English); when two
 * bundles share a language, the script is what keeps them apart, because Chinese differs by writing
 * system, not by region.
 */
function nativeLanguageName(code: LocaleCode): string {
    const pinned = LANGUAGE_LABELS[code]
    if (pinned) return pinned
    try {
        const names = new Intl.DisplayNames([code], {type: 'language'})
        const parsed = parseCulture(code)
        const name = names.of(isLoneBundleForLanguage(code) ? parsed.language : parsed.neutral)
        return name && name !== code ? name : code
    } catch {
        // No DisplayNames support: fall back to the raw code
        return code
    }
}

export const SUPPORTED_LOCALES: readonly {code: LocaleCode; label: string}[] = CODES.map((code) => ({
    code,
    label: nativeLanguageName(code),
}))

/** Fallback language: English, looked up among the shipped bundles instead of spelled out */
export const DEFAULT_LOCALE: LocaleCode =
    SUPPORTED_LOCALES.find(({code}) => parseCulture(code).language === 'en')?.code ?? 'en-US'

/** Only codes that actually ship a bundle are settable */
function isSupported(value: unknown): value is LocaleCode {
    return typeof value === 'string' && SUPPORTED_LOCALES.some(({code}) => code === value)
}

/** A shipped locale: its parsed `Intl` info plus the language code it serves */
interface AvailableCulture extends LocaleInfo {
    code: LocaleCode
}

const AVAILABLE_CULTURES: readonly AvailableCulture[] = SUPPORTED_LOCALES.map(({code}) => ({
    ...parseCulture(code),
    code,
}))

/**
 * Best shipped locale for one tag, or null so the caller can try its next preference. Most specific
 * first: the same culture once likely subtags are filled in, then the same neutral culture (same
 * language and script, region ignored). There is no language-only level on purpose — Chinese written
 * in the wrong script is a wrong answer, not a graceful fallback.
 */
function resolveCulture(raw: string): LocaleCode | null {
    const target = parseCulture(raw)
    return (
        AVAILABLE_CULTURES.find((locale) => locale.maximized === target.maximized)?.code ??
        AVAILABLE_CULTURES.find((locale) => locale.neutral === target.neutral)?.code ??
        null
    )
}

/** First match in the user's ordered preferences, else English */
function matchNavigatorLocale(): LocaleCode {
    const preferred =
        typeof navigator === 'undefined'
            ? []
            : navigator.languages?.length
                ? navigator.languages
                : [navigator.language]

    for (const culture of preferred) {
        const resolved = resolveCulture(culture)
        if (resolved) return resolved
    }
    return DEFAULT_LOCALE
}

/** A manual choice wins; a stored code that no longer ships is remapped onto its bundle */
function detectLocale(): LocaleCode {
    const stored = readLocaleSetting()
    if (stored?.manual) {
        if (isSupported(stored.locale)) return stored.locale
        const remapped = resolveCulture(stored.locale)
        if (remapped) return remapped
    }
    return matchNavigatorLocale()
}

export const i18n = createI18n({
    legacy: false,
    globalInjection: true,
    locale: detectLocale(),
    fallbackLocale: DEFAULT_LOCALE,
    messages: BUNDLES,
})

/** Mirror the language into `<HTML lang>` and the window title */
function syncDocument(locale: LocaleCode) {
    const title = i18n.global.t('app.title')
    document.documentElement.lang = locale
    document.title = title
    void setWindowTitle(title)
}

/** A Tauri window title does not follow `document.title` on its own */
async function setWindowTitle(title: string) {
    try {
        await getCurrentWindow().setTitle(title)
    } catch (err) {
        // Outside Tauri (plain browser vite dev) there is no window to rename
        console.warn('[setWindowTitle]', err)
    }
}

/** Manual switch: applies immediately and is remembered permanently */
export function setLocale(locale: LocaleCode) {
    if (!isSupported(locale) || i18n.global.locale.value === locale) return
    i18n.global.locale.value = locale
    writeLocaleSetting(locale, true)
}

watch(
    i18n.global.locale,
    (locale) => {
        syncDocument(locale as LocaleCode)
    },
    {immediate: true},
)

export default i18n
