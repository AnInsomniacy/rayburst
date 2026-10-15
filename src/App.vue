<script setup lang="ts">
/** @fileoverview Root component of the main window. */
import AppProviders from './AppProviders.vue'
import { useVisibilityPause } from './composables/useVisibilityPause'

useVisibilityPause()
</script>

<template>
  <AppProviders>
    <router-view />
  </AppProviders>
</template>

<style>
#app {
  height: 100%;
}
</style>
