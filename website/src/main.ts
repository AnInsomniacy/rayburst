/** @fileoverview Entry: fonts, global styles, the page language, then the app. */
import '@fontsource-variable/inter'
import '@fontsource-variable/jetbrains-mono'
import '@fontsource-variable/mona-sans/standard.css'
import './styles/tokens.css'
import './styles/base.css'
import './styles/controls.css'
import './styles/mock.css'
import { createApp } from 'vue'
import App from './App.vue'
import { detectLocale, i18n, setLocale } from './i18n'

// Render once the language is known, so the page never flashes English first.
await setLocale(detectLocale(), false)
createApp(App).use(i18n).mount('#app')
