/**
 * Nexus Mods GraphQL client: resolves the latest version published on this app's mod page.
 *
 * The V2 GraphQL endpoint (`https://api.nexusmods.com/v2/graphql`) serves mod metadata to
 * anonymous callers and answers with `access-control-allow-origin: *` plus a matching
 * `access-control-allow-headers: content-type` preflight, so the WebView can query it directly —
 * no API key and no Rust-side proxy are needed.
 */

import {compare} from 'compare-versions'

/** This app is published on the Baldur's Gate 3 game page (game id 3474, see `game(domainName:)`) */
const GAME_ID = 3474
const MOD_ID = 24924
const ENDPOINT = 'https://api.nexusmods.com/v2/graphql'

/** Minimal document: one mod and its version */
const MOD_RELEASE_QUERY = `
  query ModRelease($gameId: ID!, $modId: ID!) {
    mod(gameId: $gameId, modId: $modId) {
      version
    }
  }
`

/** A published release as reported by Nexus Mods */
export interface ModRelease {
    version: string
}

/** Only the fields this client reads; everything else in the payload is ignored */
interface ModReleaseResponse {
    data?: { mod?: { version?: unknown } | null }
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

    return {version}
}

/**
 * Whether `candidate` is strictly newer than `current`. `compare-versions` handles the dotted
 * forms this app publishes (including a fourth segment and a leading `v`); a version it cannot
 * parse is not treated as an update, so the About page keeps reporting "latest".
 */
export function isNewerVersion(candidate: string, current: string): boolean {
    try {
        return compare(candidate, current, '>')
    } catch {
        return false
    }
}
