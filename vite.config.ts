import {defineConfig} from 'vite'
import vue from '@vitejs/plugin-vue'
// Windows paths use backslashes, but in micromatch/anymatch "\" is an escape character, so a glob
// such as '**/src-tauri/**' never matches 'C:\...\src-tauri\...' and Vite keeps watching
// src-tauri/target — where DLLs/EXEs locked by a running Tauri process throw EBUSY and take the dev
// server down. The matcher below is therefore a function ((path: string) => boolean), a form
// anymatch supports, so paths are tested directly and the separator question never comes up.
const IGNORED_DIRS = /(^|[\\/])(src-tauri|target|dist|\.git|node_modules)([\\/]|$)/

export default defineConfig({
    plugins: [vue()],
    clearScreen: false,
    server: {
        port: 5173,
        strictPort: true,
        watch: {
            ignored: (watchPath: string) => IGNORED_DIRS.test(watchPath),
        },
    },
    build: {
        outDir: 'dist',
        emptyOutDir: true,
        // three.js ships as a single build/three.module.js (~800 KB) and cannot be split further, so
        // it is the one chunk over the default 500 KB threshold. The preview pulls it in with
        // import(), well off the startup path, so the threshold is raised rather than printing a
        // warning on every build that nothing could be done about.
        chunkSizeWarningLimit: 900,
    },
})
