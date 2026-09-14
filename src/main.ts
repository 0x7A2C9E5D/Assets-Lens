import {createApp} from 'vue'
import App from './App.vue'
import router from './router'
import i18n from './i18n'
import {startReleaseCheck} from './utils/release'
import {initTheme} from './utils/theme'
import {startWindowGeometry} from './utils/window'
import './index.css'

// Fired before the UI exists: the published version is resolved once per launch, so the About page
// arrives with the answer already known and navigation can never trigger another request
startReleaseCheck()

// Paint the remembered palette before the first frame: the window stays hidden until its geometry
// is applied, so this is the only chance to avoid a dark flash on a light-theme launch
initTheme()

createApp(App).use(router).use(i18n).mount('#app')

// The window is created hidden so the remembered rectangle can be applied before it is ever painted;
// showing it is therefore part of the boot, and it happens after the mount so the window arrives with
// its content rather than as an empty frame
startWindowGeometry()
