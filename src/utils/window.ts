/**
 * Boot step of the main window. Its rectangle is restored and persisted by the window-state plugin
 * on the Rust side (see `lib.rs`), which runs while the window is created — before the webview loads.
 *
 * The window is therefore created hidden (`"visible": false` in tauri.conf.json): showing it is the
 * last step, and it happens once the app has mounted so the window arrives with its content rather
 * than as an empty frame.
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
