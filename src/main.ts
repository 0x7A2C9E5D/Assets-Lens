import {createApp} from 'vue'
import App from './App.vue'
import router from './router'
import i18n from './i18n'
import {startReleaseCheck} from './utils/release'
import './index.css'

// Fired before the UI exists: the published version is resolved once per launch, so the About page
// arrives with the answer already known and navigation can never trigger another request
startReleaseCheck()

createApp(App).use(router).use(i18n).mount('#app')
