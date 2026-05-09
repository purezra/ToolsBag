import { createApp } from 'vue'
import './assets/styles/variables.css'
import './assets/styles/overrides.css'
import './assets/styles/style.css'
import './assets/styles/theme-dark.css'
import App from './App.vue'

// Element Plus 由 unplugin 自动按需引入，无需手动 import
const app = createApp(App)
app.mount('#app')
