/**
 * App-wide state for the "is a newer version published?" question. Resolved once per launch by
 * `startReleaseCheck()`; later calls are no-ops.
 */

import {ref} from 'vue'
import {fetchLatestRelease, isNewerVersion} from '../api/nexus'
import {getAppInfo} from '../api/tauri'
import {version as APP_VERSION} from '../../package.json'

type ReleaseState = 'idle' | 'checking' | 'latest' | 'outdated' | 'failed'

const localVersion = ref('')
const remoteVersion = ref('')
const releaseState = ref<ReleaseState>('idle')

/** Whether this launch already spent its one automatic check */
let autoCheckSpent = false

/**
 * The running version: Tauri reports it through `app_info`, with the bundled package version as a
 * fallback for plain-browser runs. Cached for the session.
 */
let localVersionRequest: Promise<string> | null = null

function resolveLocalVersion(): Promise<string> {
    localVersionRequest ??= getAppInfo()
        .then((info) => info.version)
        .catch(() => APP_VERSION)
    return localVersionRequest
}

/** The single automatic check of this launch; every later call does nothing */
export function startReleaseCheck(): void {
    if (autoCheckSpent) return
    autoCheckSpent = true

    releaseState.value = 'checking'
    void resolveLocalVersion()
        .then(async (current) => {
            // Published before the request: the badge is local information and must not wait on the network
            localVersion.value = current
            const release = await fetchLatestRelease()
            remoteVersion.value = release.version
            releaseState.value = isNewerVersion(release.version, current) ? 'outdated' : 'latest'
        })
        .catch((err) => {
            // The page renders nothing for a failed check, so the console is where it surfaces
            console.error('[nexus_release]', err)
            releaseState.value = 'failed'
        })
}

/** Read-only view of the shared state; the check itself is driven by the app, not the page. */
export function useReleaseCheck() {
    return {localVersion, remoteVersion, releaseState}
}
