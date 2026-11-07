/**
 * Boot step of the main window. The window is created hidden (`"visible": false` in
 * tauri.conf.json), so showing it is the last step of the boot and it arrives with its content.
 */

import {getCurrentWindow} from '@tauri-apps/api/window'

/** Reveal the window after the first frame has been painted */
export function showMainWindow(): void {
    void getCurrentWindow()
        .show()
        .catch((err) => {
            // A failed show would leave an invisible app with no trace at all
            console.error('[window]', err)
        })
}
