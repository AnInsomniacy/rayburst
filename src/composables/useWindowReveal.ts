/**
 * @fileoverview Timing for windows that animate their content in as they appear.
 *
 * A window that was hidden resumes painting a few frames after it is shown. A
 * transition started at once would spend those frames unseen and appear to
 * jump; it starts instead once the shown page has painted its start state.
 */

/** Resolves on the next animation frame. */
export function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()))
}

/** Longest wait for the page to report itself visible after its window is shown. */
const VISIBLE_TIMEOUT_MS = 250

function whenVisible(): Promise<void> {
  if (document.visibilityState === 'visible') return Promise.resolve()
  return new Promise((resolve) => {
    const done = () => {
      clearTimeout(timer)
      document.removeEventListener('visibilitychange', onChange)
      resolve()
    }
    const onChange = () => {
      if (document.visibilityState === 'visible') done()
    }
    const timer = setTimeout(done, VISIBLE_TIMEOUT_MS)
    document.addEventListener('visibilitychange', onChange)
  })
}

/** Resolves once a just-shown window has painted, so its entrance can start. */
export async function afterReveal(): Promise<void> {
  await whenVisible()
  await nextFrame()
  await nextFrame()
}

/** Whether the user asked for reduced motion. */
export function reducedMotion(): boolean {
  return document.documentElement.classList.contains('reduce-motion')
}
