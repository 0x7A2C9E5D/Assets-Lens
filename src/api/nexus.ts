/**
 * Nexus Mods GraphQL client: resolves the latest version published on this app's mod page.
 *
 * The V2 GraphQL endpoint (`https://api.nexusmods.com/v2/graphql`) serves mod metadata to
 * anonymous callers and answers with `access-control-allow-origin: *` plus a matching
 * `access-control-allow-headers: content-type` preflight, so the WebView can query it directly —
 * no API key and no Rust-side proxy are needed.
 */

/** This app is published on the Baldur's Gate 3 game page (game id 3474, see `game(domainName:)`) */
const GAME_ID = 3474
const MOD_ID = 24924
const ENDPOINT = 'https://api.nexusmods.com/v2/graphql'

/** Minimal document: one mod, its version and when it was last updated */
const MOD_RELEASE_QUERY = `
  query ModRelease($gameId: ID!, $modId: ID!) {
    mod(gameId: $gameId, modId: $modId) {
      version
      updatedAt
    }
  }
`

/** A published release as reported by Nexus Mods */
export interface ModRelease {
    version: string
    /** ISO 8601 timestamp of the last update, empty when the API omits it */
    updatedAt: string
}

/** Only the fields this client reads; everything else in the payload is ignored */
interface ModReleaseResponse {
    data?: { mod?: { version?: unknown; updatedAt?: unknown } | null }
    errors?: { message?: string }[]
}

/**
 * Fetch the published release. Deliberately uncached: the check is user-initiated, so answering
 * from a stale local copy would contradict what the button promises. A manual click is far too
 * infrequent to matter for the anonymous rate limit.
 */
export async function fetchLatestRelease(): Promise<ModRelease> {
    const response = await fetch(ENDPOINT, {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify({
            query: MOD_RELEASE_QUERY,
            variables: {gameId: GAME_ID, modId: MOD_ID},
        }),
    })
    if (!response.ok) {
        throw new Error(`Nexus Mods API responded with HTTP ${response.status}`)
    }

    const payload: ModReleaseResponse = await response.json()
    const version = payload.data?.mod?.version
    if (typeof version !== 'string' || !version) {
        const reason = payload.errors?.[0]?.message ?? 'the response carried no version'
        throw new Error(`Nexus Mods API returned no version: ${reason}`)
    }

    const updatedAt = payload.data?.mod?.updatedAt
    return {
        version,
        updatedAt: typeof updatedAt === 'string' ? updatedAt : '',
    }
}

/**
 * Compare two dotted versions as numbers, so 0.10.0 counts as newer than 0.9.0. Any pre-release
 * or build suffix (`1.2.0-beta.3`) is ignored. Returns true only when `candidate` is strictly newer.
 */
export function isNewerVersion(candidate: string, current: string): boolean {
    const next = versionParts(candidate)
    const local = versionParts(current)
    for (let i = 0; i < Math.max(next.length, local.length); i += 1) {
        const diff = (next[i] ?? 0) - (local[i] ?? 0)
        if (diff !== 0) return diff > 0
    }
    return false
}

/** Split a version into its numeric segments ("v1.2.3" → [1, 2, 3]) */
function versionParts(version: string): number[] {
    return version
        .replace(/^v/i, '')
        .split(/[.\-_+]/)
        .map((part) => Number.parseInt(part, 10))
        .filter((part) => Number.isFinite(part))
}
