/**
 * @fileoverview Staggered entrance of a dialog's content.
 *
 * The modal scales in as a whole; its sections then rise in one after another
 * (`.entrance` items in `styles/transitions.css`, ordered by `--entrance`).
 * The sequence restarts every time the dialog opens.
 */
import { ref, watch, type Ref } from 'vue'

export function usePanelEntrance(show: () => boolean): Ref<boolean> {
  const entering = ref(false)
  watch(
    show,
    (visible) => {
      if (!visible) return
      entering.value = false
      requestAnimationFrame(() => {
        entering.value = true
      })
    },
    { immediate: true },
  )
  return entering
}
