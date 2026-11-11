import { changeOption, fetchTaskList, getOption, saveSession } from '@/api/aria2'

const TASK_KEYS = ['stream-max-connections', 'max-download-limit', 'max-upload-limit'] as const

/** Capture effective options before global defaults change; rollback preserves per-task overrides. */
export async function prepareTaskPreferenceOptions(options: Record<string, string>) {
  const changed = Object.fromEntries(Object.entries(options).filter(([key]) => TASK_KEYS.some((k) => k === key)))
  const snapshots: { gid: string; options: Record<string, string> }[] = []
  const attempted: typeof snapshots = []
  if (Object.keys(changed).length > 0) {
    // The native active snapshot includes waiting and paused tasks.
    const tasks = await fetchTaskList({ type: 'active' })
    for (const gid of new Set(tasks.map((task) => task.gid))) {
      const current = await getOption({ gid })
      const previous: Record<string, string> = {}
      for (const key of Object.keys(changed)) {
        const camelKey = key.replace(/-([a-z])/g, (_, letter: string) => letter.toUpperCase())
        const value = current[camelKey]
        if (value === undefined) throw new Error(`Missing task option ${key} for ${gid}`)
        previous[key] = value
      }
      snapshots.push({ gid, options: previous })
    }
  }
  return {
    async apply() {
      for (const snapshot of snapshots) {
        // Include a failing request: the engine may apply it before returning an error.
        attempted.push(snapshot)
        await changeOption({ gid: snapshot.gid, options: changed })
      }
      if (attempted.length > 0) await saveSession()
    },
    async rollback() {
      if (attempted.length === 0) return
      const failures: unknown[] = []
      for (const snapshot of attempted) {
        try {
          await changeOption(snapshot)
        } catch (error) {
          failures.push(error)
        }
      }
      try {
        await saveSession()
      } catch (error) {
        failures.push(error)
      }
      if (failures.length > 0) throw new AggregateError(failures, 'Task option rollback failed')
    },
  }
}
