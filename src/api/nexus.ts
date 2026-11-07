/**
 * Nexus Mods GraphQL client: resolves the latest version published on this app's mod page. The V2
 * endpoint allows anonymous cross-origin requests, so no API key or Rust proxy is needed.
 */

import {compare} from 'compare-versions'

/** Published on the Baldur's Gate 3 page (game id 3474) */
const GAME_ID = 3474
const MOD_ID = 26506
const ENDPOINT = 'https://api.nexusmods.com/v2/graphql'

/** The app's own mod page, built from the same id the release check queries */
export const MOD_PAGE_URL = `https://www.nexusmods.com/baldursgate3/mods/${MOD_ID}`

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
 * Fetch the published release. Uncached: the check is user-initiated and infrequent enough for the
 * anonymous rate limit.
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
 * Whether `candidate` is strictly newer than `current`; an unparsable version is not an update.
 */
export function isNewerVersion(candidate: string, current: string): boolean {
    try {
        return compare(candidate, current, '>')
    } catch {
        return false
    }
}
