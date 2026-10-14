<script setup lang="ts">
/** @fileoverview Naive UI theme, locale, message and dialog providers shared by every window. */
import { computed, provide, watch, watchEffect } from 'vue'
import { useI18n } from 'vue-i18n'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import {
  NConfigProvider,
  NMessageProvider,
  NDialogProvider,
  darkTheme,
  type NLocale,
  type NDateLocale,
  zhCN,
  zhTW,
  jaJP,
  koKR,
  ruRU,
  frFR,
  deDE,
  esAR,
  ptBR,
  itIT,
  trTR,
  idID,
  viVN,
  plPL,
  thTH,
  arDZ,
  ukUA,
  nlNL,
  nbNO,
  dateZhCN,
  dateZhTW,
  dateJaJP,
  dateKoKR,
  dateRuRU,
  dateFrFR,
  dateDeDE,
  dateEsAR,
  datePtBR,
  dateItIT,
  dateTrTR,
  dateIdID,
  dateViVN,
  datePlPL,
  dateThTH,
  dateArDZ,
  dateUkUA,
  dateNlNL,
  dateNbNO,
} from 'naive-ui'
import { useTheme } from './composables/useTheme'
import { useReducedMotionClass } from './composables/useReducedMotion'
import { useInterfaceScale } from './composables/useInterfaceScale'
import { usePreferenceStore } from './stores/preference'
import { setIecSizeUnits } from '@shared/utils/format'
import { isSupportedLocale, type SupportedLocale } from '@shared/localeCatalog'
import { logger } from '@shared/logger'

import { APP_COLOR_TOKENS_KEY, useColorScheme } from './composables/useColorScheme'

const { locale: currentLocale } = useI18n()
const { isDark } = useTheme()
const { colorTokens, themeOverrides } = useColorScheme()
provide(APP_COLOR_TOKENS_KEY, colorTokens)
useReducedMotionClass()
useInterfaceScale()
const preferenceStore = usePreferenceStore()
watchEffect(() => setIecSizeUnits(preferenceStore.config.iecSizeUnits))

const theme = computed(() => (isDark.value ? darkTheme : null))

// Native window and webview backgrounds show wherever the page has not
// painted yet, as while a window resizes; keep them in the page's color
// (`--main-bg` in tokens.css) so they never flash white.
const windowBackground = computed(() =>
  isDark.value ? colorTokens.value.surfaceContainerHigh : colorTokens.value.surfaceContainerLow,
)
watch(
  windowBackground,
  (color) => {
    getCurrentWebviewWindow()
      .setBackgroundColor(color)
      .catch((error: unknown) => logger.debug('AppProviders.setBackgroundColor', error))
  },
  { immediate: true },
)

const naiveLocaleMap: Partial<Record<SupportedLocale, NLocale>> = {
  'zh-CN': zhCN,
  'zh-TW': zhTW,
  ja: jaJP,
  ko: koKR,
  ru: ruRU,
  fr: frFR,
  de: deDE,
  es: esAR,
  'pt-BR': ptBR,
  it: itIT,
  tr: trTR,
  id: idID,
  vi: viVN,
  pl: plPL,
  th: thTH,
  ar: arDZ,
  uk: ukUA,
  nl: nlNL,
  nb: nbNO,
}
const naiveDateLocaleMap: Partial<Record<SupportedLocale, NDateLocale>> = {
  'zh-CN': dateZhCN,
  'zh-TW': dateZhTW,
  ja: dateJaJP,
  ko: dateKoKR,
  ru: dateRuRU,
  fr: dateFrFR,
  de: dateDeDE,
  es: dateEsAR,
  'pt-BR': datePtBR,
  it: dateItIT,
  tr: dateTrTR,
  id: dateIdID,
  vi: dateViVN,
  pl: datePlPL,
  th: dateThTH,
  ar: dateArDZ,
  uk: dateUkUA,
  nl: dateNlNL,
  nb: dateNbNO,
}

const naiveLocale = computed(() =>
  isSupportedLocale(currentLocale.value) ? naiveLocaleMap[currentLocale.value] || null : null,
)
const naiveDateLocale = computed(() =>
  isSupportedLocale(currentLocale.value) ? naiveDateLocaleMap[currentLocale.value] || null : null,
)
</script>

<template>
  <NConfigProvider
    :theme="theme"
    :theme-overrides="themeOverrides"
    :locale="naiveLocale"
    :date-locale="naiveDateLocale"
  >
    <NMessageProvider>
      <NDialogProvider>
        <slot />
      </NDialogProvider>
    </NMessageProvider>
  </NConfigProvider>
</template>
