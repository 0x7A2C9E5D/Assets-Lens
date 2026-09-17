/**
 * The window rectangle (size and position) as remembered across launches. It is a setting like any
 * other, so it lives in localStorage next to the game directory (see `settings.ts`) and the backend
 * is not involved.
 *
 * Restoring it has to wait until the webview is alive, so the window is created hidden
 * (`"visible": false` in tauri.conf.json) and shown here, once the rectangle is in place: showing it
 * first would turn every launch into a visible jump.
 *
 * Only the window's normal rectangle is remembered. A maximized window reports the work area, which
 * is not the size it returns to when un-maximized, and a minimized one is parked far outside every
 * monitor; in both cases the rectangle is left alone and only the `maximized` flag is carried along.
 */

import {PhysicalPosition, PhysicalSize} from '@tauri-apps/api/dpi'
import {availableMonitors, getCurrentWindow, type Monitor} from '@tauri-apps/api/window'
import {readWindowGeometry, type WindowGeometry, writeWindowGeometry} from './settings'

/** A rectangle without the `maximized` flag: the half of the record the move/resize events carry */
type Rect = Omit<WindowGeometry, 'maximized'>

/** The last rectangle seen while the window was in its normal state */
let lastNormal: WindowGeometry | null = null

/**
 * Boot: put the window where the last session left it, show it, then follow it. Failures are
 * reported to the console rather than thrown at the caller — the window is shown before anything but
 * the restore itself can fail, and a failed restore only costs the remembered rectangle.
 */
export function startWindowGeometry(): void {
    void boot().catch((err) => {
        console.error('[window_geometry]', err)
    })
}

async function boot(): Promise<void> {
    try {
        const remembered = readWindowGeometry()
        if (remembered) await place(remembered)
            // Nothing remembered yet (first launch): the rectangle the window was created with is the one
        // to come back to
        else await seed()
    } catch (err) {
        // A restore that fails (a denied permission, an unavailable monitor list) must not leave the
        // window hidden, so the show below runs regardless
        console.error('[window_geometry]', err)
    }

    await getCurrentWindow().show()
    await follow()
}

/** Move and size the window to `geometry`, and start the record from it */
async function place(geometry: WindowGeometry): Promise<void> {
    const appWindow = getCurrentWindow()

    if (await isOnScreen(geometry)) {
        await appWindow.setPosition(new PhysicalPosition(geometry.x, geometry.y))
    }
    await appWindow.setSize(new PhysicalSize(geometry.width, geometry.height))
    if (geometry.maximized) await appWindow.maximize()

    // Seed the record with what was just applied: a window that comes back maximized reports no
    // normal rectangle of its own, so this is what keeps the remembered size alive until the user
    // moves the window. It is already what storage holds, hence no write.
    lastNormal = geometry
}

/** Nothing remembered yet: the rectangle the window was created with is the one to come back to */
async function seed(): Promise<void> {
    const rect = await currentRect()
    remember({...rect, maximized: false})
}

/** Follow the window's moves and resizes, and record what they report */
async function follow(): Promise<void> {
    const appWindow = getCurrentWindow()
    // The payloads carry the half of the rectangle that changed; the window state itself still has to
    // be asked about, to tell a normal window from a maximized or minimized one
    await appWindow.onMoved(({payload: {x, y}}) => void track({x, y}))
    await appWindow.onResized(({payload: {width, height}}) => void track({width, height}))
}

async function track(changed: Partial<Rect>): Promise<void> {
    try {
        const appWindow = getCurrentWindow()
        const [maximized, minimized] = await Promise.all([
            appWindow.isMaximized(),
            appWindow.isMinimized(),
        ])
        if (maximized || minimized) {
            // Neither rectangle is one to come back to, but the flag still has to travel: the next
            // launch has to know the window was left maximized
            if (maximized && lastNormal) remember({...lastNormal, maximized: true})
            return
        }
        // Without a rectangle to start from there is nothing to update; a partial record would be
        // worse than none at all
        if (!lastNormal) return
        remember({...lastNormal, ...changed, maximized: false})
    } catch (err) {
        console.error('[window_geometry]', err)
    }
}

/** Take `geometry` as the current record and write it out */
function remember(geometry: WindowGeometry): void {
    // Events fire far more often than the rectangle changes (a maximized window keeps reporting the
    // same one), so an unchanged record is dropped instead of written again
    if (lastNormal && isSameRect(lastNormal, geometry)) return
    lastNormal = geometry
    writeWindowGeometry(geometry)
}

function isSameRect(a: WindowGeometry, b: WindowGeometry): boolean {
    return (
        a.x === b.x &&
        a.y === b.y &&
        a.width === b.width &&
        a.height === b.height &&
        a.maximized === b.maximized
    )
}

/** The window's rectangle right now, in physical pixels */
async function currentRect(): Promise<Rect> {
    const appWindow = getCurrentWindow()
    const [position, size] = await Promise.all([appWindow.outerPosition(), appWindow.innerSize()])
    return {x: position.x, y: position.y, width: size.width, height: size.height}
}

/**
 * Whether `geometry` still overlaps a monitor. A remembered position can outlive the screen it was
 * taken on (a monitor went away, the resolution changed) and a window restored off screen could not
 * be dragged back, so the position is dropped while the size is kept.
 */
async function isOnScreen(geometry: WindowGeometry): Promise<boolean> {
    try {
        const monitors = await availableMonitors()
        if (!monitors.length) return true
        return monitors.some((monitor) => overlapsMonitor(geometry, monitor))
    } catch {
        // No answer: keep the remembered position rather than move the window somewhere arbitrary
        return true
    }
}

function overlapsMonitor(geometry: WindowGeometry, monitor: Monitor): boolean {
    const {position, size} = monitor
    return (
        geometry.x < position.x + size.width &&
        geometry.x + geometry.width > position.x &&
        geometry.y < position.y + size.height &&
        geometry.y + geometry.height > position.y
    )
}
