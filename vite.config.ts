import {defineConfig} from 'vite'
import vue from '@vitejs/plugin-vue'
// A glob such as '**/src-tauri/**' never matches Windows paths (backslash is an escape character to
// micromatch), so the ignore matcher is a function: locked DLLs in src-tauri/target cannot take the
// dev server down with EBUSY.
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
        // three.js ships as one ~800 KB module; it is lazy-loaded by the preview, well off the
        // startup path, so the warning threshold is raised rather than flagging every build.
        chunkSizeWarningLimit: 900,
    },
})
