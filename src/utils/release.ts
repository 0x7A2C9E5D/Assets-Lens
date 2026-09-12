/**
 * App-wide state for the "is a newer version published?" question.
 *
 * The published version is resolved exactly once per launch: `startReleaseCheck()` runs while the
 * app boots and turns into a no-op afterward, so moving between routes (the About page is rebuilt
 * on every visit) can never spend a second request.
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
 * The running version: Tauri reports it at runtime through `app_info`, while the bundled package
 * version covers plain-browser runs where that command does not exist. Cached for the session
 * because neither answer can change while the app is open.
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

/**
 * Read-only view of the shared state; the check itself is driven by the app, not by the page.
 * A failure reason is not exposed — the page either shows the update arrow or stays empty.
 */
export function useReleaseCheck() {
  return {localVersion, remoteVersion, releaseState}
}
