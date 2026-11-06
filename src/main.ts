import {createApp} from 'vue'
import App from './App.vue'
import router from './router'
import i18n from './i18n'
import {startReleaseCheck} from './utils/release'
// Imported for its side effect: it reads the stored palette and paints <html> before the first frame
import './utils/theme'
import {showMainWindow} from './utils/window'
import './index.css'

// Fired before the UI exists: the published version is resolved once per launch, so the About page
// arrives with the answer already known and navigation can never trigger another request
startReleaseCheck()

createApp(App).use(router).use(i18n).mount('#app')

// The window is created hidden so the plugin-restored rectangle is in place before it is ever
// painted; showing it is the last step of the boot, after the mount, so the window arrives with its
// content rather than as an empty frame
showMainWindow()
