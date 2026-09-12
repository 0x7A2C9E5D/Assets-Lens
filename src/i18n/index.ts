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
    /** Canonical tag, e.g. `zh-CN` */
    name: string
    /** Tag with likely subtags filled in, e.g. `zh-Hans-CN` */
    maximized: string
    /** Language plus script, region dropped, e.g. `zh-Hans` */
    neutral: string
    /** Bare language subtag, e.g. `zh` */
    language: string
}

/** Matching rules, so the lookup below depends on the contract and not on the `Intl` implementation */
interface CultureMatcher {
    matches<L extends LocaleInfo>(targetLocale: LocaleInfo, availableLocales: readonly L[]): L[]
}

/**
 * Parse an OS/browser locale tag. A malformed tag degrades to the cleaned string instead of
 * throwing, so callers never have to guard the result.
 */
function parseCulture(raw: string): LocaleInfo {
    // Clean up first: Intl rejects POSIX-style tags such as `zh_CN.UTF-8` outright
    const cleaned = raw.trim().toLowerCase().replace(/_/g, '-').split(/[.@]/)[0]
    try {
        const locale = new Intl.Locale(cleaned)
        const maximized = locale.maximize()
        const script = maximized.script
        return {
            name: locale.baseName.toLowerCase(),
            maximized: maximized.baseName.toLowerCase(),
            neutral: (script ? `${maximized.language}-${script}` : maximized.language).toLowerCase(),
            language: maximized.language.toLowerCase(),
        }
    } catch {
        // Malformed tag, or an engine without Intl.Locale
        const language = cleaned.split('-')[0]
        return {name: cleaned, maximized: cleaned, neutral: language, language}
    }
}

/**
 * Most specific first: the same culture once likely subtags are filled in, then the same neutral
 * culture (same language and script, region ignored). There is no language-only level on purpose —
 * Chinese written in the wrong script is a wrong answer, not a graceful fallback.
 */
class IntlCultureMatcher implements CultureMatcher {
    matches<L extends LocaleInfo>(
        targetLocale: LocaleInfo,
        availableLocales: readonly L[],
    ): L[] {
        const find = (predicate: (locale: L) => boolean): L[] | null => {
            const hits = availableLocales.filter(predicate)
            return hits.length > 0 ? hits : null
        }

        return (
            find((locale) => locale.maximized === targetLocale.maximized) ??
            find((locale) => locale.neutral === targetLocale.neutral) ??
            []
        )
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
 * A language's own name, from `Intl` (`zh-CN` → 中文) rather than a hand-kept label table. The bare
 * language is preferred because the regional form is wordy (`en-US` → American English); when two
 * bundles share a language, the full tag is what keeps them apart (`中文（中国）` vs `中文（台灣）`).
 */
function nativeLanguageName(code: LocaleCode): string {
    try {
        const names = new Intl.DisplayNames([code], {type: 'language'})
        const name = names.of(isLoneBundleForLanguage(code) ? parseCulture(code).language : code)
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
    SUPPORTED_LOCALES.find(({code}) => parseCulture(code).language === 'en')?.code ??
    CODES[0] ??
    'en-US'

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

const cultureMatcher: CultureMatcher = new IntlCultureMatcher()

/** Best shipped locale for one tag, or null so the caller can try its next preference */
function resolveCulture(raw: string): LocaleCode | null {
    const [best] = cultureMatcher.matches(parseCulture(raw), AVAILABLE_CULTURES)
    return best?.code ?? null
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

/** Mirror the language into `<html lang>` and the window title */
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
