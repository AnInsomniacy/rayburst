/** @fileoverview Reactive Material color tokens shared by CSS, Naive UI, and Canvas consumers. */
import { computed, inject, watchEffect, type ComputedRef, type InjectionKey } from 'vue'
import { usePreferenceStore } from '@/stores/preference'
import { useTheme } from '@/composables/useTheme'
import { type ColorSchemeDefinition } from '@shared/constants'
import {
  buildAppColorTokens,
  buildColorSchemeTheme,
  resolveColorScheme,
  type AppColorTokens,
} from '@shared/utils/colorScheme'
import type { GlobalThemeOverrides } from 'naive-ui'

export const APP_COLOR_TOKENS_KEY: InjectionKey<ComputedRef<AppColorTokens>> = Symbol('app-color-tokens')

const MESSAGE_SHADOW = '0 2px 8px var(--m3-shadow)'

export function buildCssVariables(tokens: AppColorTokens): Record<string, string> {
  return {
    '--m3-primary': tokens.primary.color,
    '--m3-on-primary': tokens.primary.onColor,
    '--m3-primary-container': tokens.primary.container,
    '--m3-on-primary-container': tokens.primary.onContainer,
    '--m3-tertiary': tokens.tertiary.color,
    '--m3-on-tertiary': tokens.tertiary.onColor,
    '--m3-tertiary-container': tokens.tertiary.container,
    '--m3-on-tertiary-container': tokens.tertiary.onContainer,
    '--m3-info': tokens.info.color,
    '--m3-on-info': tokens.info.onColor,
    '--m3-info-container': tokens.info.container,
    '--m3-on-info-container': tokens.info.onContainer,
    '--m3-success': tokens.success.color,
    '--m3-on-success': tokens.success.onColor,
    '--m3-success-container': tokens.success.container,
    '--m3-on-success-container': tokens.success.onContainer,
    '--m3-warning': tokens.warning.color,
    '--m3-on-warning': tokens.warning.onColor,
    '--m3-warning-container': tokens.warning.container,
    '--m3-on-warning-container': tokens.warning.onContainer,
    '--m3-error': tokens.error.color,
    '--m3-on-error': tokens.error.onColor,
    '--m3-error-container': tokens.error.container,
    '--m3-on-error-container': tokens.error.onContainer,
    '--m3-surface': tokens.surface,
    '--m3-on-surface': tokens.onSurface,
    '--m3-on-surface-variant': tokens.onSurfaceVariant,
    '--m3-surface-dim': tokens.surfaceDim,
    '--m3-surface-container-lowest': tokens.surfaceContainerLowest,
    '--m3-surface-container-low': tokens.surfaceContainerLow,
    '--m3-surface-container': tokens.surfaceContainer,
    '--m3-surface-container-high': tokens.surfaceContainerHigh,
    '--m3-surface-container-highest': tokens.surfaceContainerHighest,
    '--m3-outline': tokens.outline,
    '--m3-outline-variant': tokens.outlineVariant,
    '--m3-inverse-surface': tokens.inverseSurface,
    '--m3-on-inverse-surface': tokens.onInverseSurface,
    '--m3-scrollbar-thumb': `color-mix(in srgb, ${tokens.onSurface} 24%, transparent)`,
    '--m3-scrollbar-thumb-inactive': `color-mix(in srgb, ${tokens.onSurface} 12%, transparent)`,
    '--m3-status-active': tokens.statusActive,
    '--m3-status-waiting': tokens.statusWaiting,
    '--m3-status-paused': tokens.statusPaused,
    '--m3-status-error': tokens.statusError,
    '--m3-status-success': tokens.statusSuccess,
  }
}

export function buildNaiveTheme(tokens: AppColorTokens): GlobalThemeOverrides {
  const { primary, info, success, warning, error } = tokens
  return {
    common: {
      primaryColor: primary.color,
      primaryColorHover: primary.hover,
      primaryColorPressed: primary.pressed,
      primaryColorSuppl: primary.color,
      infoColor: info.color,
      infoColorHover: info.hover,
      infoColorPressed: info.pressed,
      infoColorSuppl: info.color,
      successColor: success.color,
      successColorHover: success.hover,
      successColorPressed: success.pressed,
      successColorSuppl: success.color,
      warningColor: warning.color,
      warningColorHover: warning.hover,
      warningColorPressed: warning.pressed,
      warningColorSuppl: warning.color,
      errorColor: error.color,
      errorColorHover: error.hover,
      errorColorPressed: error.pressed,
      errorColorSuppl: error.color,
      textColorBase: tokens.onSurface,
      textColor1: tokens.onSurface,
      textColor2: tokens.onSurface,
      textColor3: tokens.onSurfaceVariant,
      placeholderColor: tokens.onSurfaceVariant,
      tableColor: tokens.surfaceContainer,
      tableHeaderColor: tokens.surfaceContainerLow,
      tableColorHover: tokens.surfaceContainerHigh,
      tableColorStriped: tokens.surfaceContainerLow,
      actionColor: tokens.surfaceContainerLow,
      tabColor: tokens.surfaceContainerLow,
      codeColor: tokens.surfaceContainerLow,
      tagColor: tokens.surfaceContainerHigh,
      avatarColor: tokens.surfaceContainerHigh,
      hoverColor: tokens.surfaceContainerHigh,
      pressedColor: tokens.surfaceContainerHighest,
      railColor: tokens.outlineVariant,
      progressRailColor: tokens.outlineVariant,
      inputColor: tokens.surfaceContainer,
      inputColorDisabled: tokens.surfaceContainerLow,
      bodyColor: 'transparent',
      cardColor: tokens.surfaceContainer,
      modalColor: tokens.surfaceContainerHigh,
      popoverColor: tokens.surfaceContainerHighest,
      borderColor: tokens.outlineVariant,
      dividerColor: tokens.outlineVariant,
      borderRadius: '6px',
      fontFamily:
        '"Monospaced Number", "Chinese Quote", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", system-ui, sans-serif',
    },
    Tooltip: {
      color: tokens.inverseSurface,
      textColor: tokens.onInverseSurface,
    },
    Checkbox: {
      color: tokens.surfaceContainer,
      colorChecked: primary.color,
      checkMarkColor: primary.onColor,
    },
    Alert: {
      colorInfo: info.container,
      colorSuccess: success.container,
      colorWarning: warning.container,
      colorError: error.container,
      borderInfo: `1px solid ${info.color}`,
      borderSuccess: `1px solid ${success.color}`,
      borderWarning: `1px solid ${warning.color}`,
      borderError: `1px solid ${error.color}`,
      titleTextColorInfo: info.onContainer,
      contentTextColorInfo: info.onContainer,
      titleTextColorSuccess: success.onContainer,
      contentTextColorSuccess: success.onContainer,
      titleTextColorWarning: warning.onContainer,
      contentTextColorWarning: warning.onContainer,
      titleTextColorError: error.onContainer,
      contentTextColorError: error.onContainer,
    },
    Divider: {
      color: tokens.outlineVariant,
    },
    Button: {
      border: `1px solid ${tokens.outlineVariant}`,
      borderHover: `1px solid ${tokens.outline}`,
      borderFocus: `1px solid ${tokens.outline}`,
      textColorPrimary: primary.onColor,
      textColorHoverPrimary: primary.onColor,
      textColorPressedPrimary: primary.onColor,
      textColorFocusPrimary: primary.onColor,
      textColorInfo: info.onColor,
      textColorHoverInfo: info.onColor,
      textColorPressedInfo: info.onColor,
      textColorFocusInfo: info.onColor,
      textColorSuccess: success.onColor,
      textColorHoverSuccess: success.onColor,
      textColorPressedSuccess: success.onColor,
      textColorFocusSuccess: success.onColor,
      textColorWarning: warning.onColor,
      textColorHoverWarning: warning.onColor,
      textColorPressedWarning: warning.onColor,
      textColorFocusWarning: warning.onColor,
      textColorError: error.onColor,
      textColorHoverError: error.onColor,
      textColorPressedError: error.onColor,
      textColorFocusError: error.onColor,
    },
    Input: {
      color: tokens.surfaceContainer,
      colorFocus: tokens.surfaceContainer,
      textColor: tokens.onSurface,
      placeholderColor: tokens.onSurfaceVariant,
      border: `1px solid ${tokens.outlineVariant}`,
      borderHover: `1px solid ${tokens.outline}`,
      borderFocus: `1px solid ${primary.color}`,
    },
    InputNumber: {
      peers: {
        Input: {
          color: tokens.surfaceContainer,
          colorFocus: tokens.surfaceContainer,
          textColor: tokens.onSurface,
          border: `1px solid ${tokens.outlineVariant}`,
          borderHover: `1px solid ${tokens.outline}`,
          borderFocus: `1px solid ${primary.color}`,
        },
        Button: {
          textColor: tokens.onSurfaceVariant,
          textColorHover: tokens.onSurface,
        },
      },
    },
    Card: {
      color: tokens.surfaceContainerLow,
      textColor: tokens.onSurface,
      titleTextColor: tokens.onSurface,
      borderColor: tokens.outlineVariant,
    },
    Message: {
      // Stacked toasts need a visible gap: the default 8px is filled by Naive's wide
      // shadow, so a tight shadow and an outline keep each toast distinct.
      margin: '0 0 10px 0',
      border: `1px solid ${tokens.outlineVariant}`,
      boxShadow: MESSAGE_SHADOW,
      boxShadowInfo: MESSAGE_SHADOW,
      boxShadowSuccess: MESSAGE_SHADOW,
      boxShadowWarning: MESSAGE_SHADOW,
      boxShadowError: MESSAGE_SHADOW,
      boxShadowLoading: MESSAGE_SHADOW,
      // Toasts float above the window: lighter than it in both modes.
      color: tokens.surfaceRaised,
      textColor: tokens.onSurface,
      closeIconColor: tokens.onSurfaceVariant,
      closeIconColorHover: tokens.onSurface,
      colorInfo: tokens.surfaceRaised,
      colorSuccess: tokens.surfaceRaised,
      colorWarning: tokens.surfaceRaised,
      colorError: tokens.surfaceRaised,
    },
    Dialog: {
      color: tokens.surfaceContainerHigh,
      textColor: tokens.onSurface,
      titleTextColor: tokens.onSurface,
    },
    Drawer: {
      color: tokens.surfaceContainerHigh,
      textColor: tokens.onSurface,
      titleTextColor: tokens.onSurface,
    },
    Radio: {
      color: tokens.surfaceContainer,
      buttonColor: tokens.surfaceContainer,
      buttonColorActive: primary.container,
      buttonTextColor: tokens.onSurface,
      buttonTextColorActive: primary.onContainer,
      buttonTextColorHover: primary.color,
    },
    Switch: {
      railColorActive: primary.color,
    },
    Tabs: {
      tabColorSegment: tokens.surfaceContainer,
      tabTextColorActiveLine: tokens.onSurface,
      tabTextColorActiveBar: tokens.onSurface,
      tabTextColorActiveCard: tokens.onSurface,
      tabTextColorHoverLine: tokens.onSurface,
      tabTextColorHoverBar: tokens.onSurface,
      tabTextColorHoverCard: tokens.onSurface,
      barColor: primary.color,
    },
    Tag: {
      textColorCheckable: tokens.onSurfaceVariant,
      textColorHoverCheckable: primary.color,
      textColorChecked: primary.onColor,
      colorChecked: primary.color,
      colorCheckedHover: primary.color,
    },
    Select: {
      peers: {
        InternalSelection: {
          border: `1px solid ${tokens.outlineVariant}`,
          borderHover: `1px solid ${tokens.outline}`,
          borderFocus: `1px solid ${primary.color}`,
          borderActive: `1px solid ${primary.color}`,
        },
        InternalSelectMenu: {
          optionTextColorActive: tokens.onSurface,
          optionTextColorPressed: tokens.onSurface,
          optionCheckColor: primary.color,
          optionColorActive: `color-mix(in srgb, ${primary.color} 10%, transparent)`,
          optionColorActivePending: `color-mix(in srgb, ${primary.color} 14%, transparent)`,
        },
      },
    },
  }
}

export function useColorScheme() {
  const preferenceStore = usePreferenceStore()
  const { isDark } = useTheme()

  const currentScheme = computed<ColorSchemeDefinition>(() =>
    resolveColorScheme(preferenceStore.config.colorScheme, preferenceStore.config.customColorScheme),
  )
  const materialTheme = computed(() => buildColorSchemeTheme(currentScheme.value))
  const colorTokens = computed(() => buildAppColorTokens(materialTheme.value, isDark.value))
  const themeOverrides = computed(() => buildNaiveTheme(colorTokens.value))

  watchEffect(() => {
    const root = document.documentElement.style
    for (const [name, value] of Object.entries(buildCssVariables(colorTokens.value))) {
      root.setProperty(name, value)
    }
  })

  return { currentScheme, colorTokens, themeOverrides }
}

export function useAppColorTokens(): ComputedRef<AppColorTokens> {
  const tokens = inject(APP_COLOR_TOKENS_KEY)
  if (!tokens) throw new Error('App color tokens are unavailable')
  return tokens
}
