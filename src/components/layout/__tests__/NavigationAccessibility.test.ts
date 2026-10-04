import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent, reactive, ref } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'
import AppSidebar from '../AppSidebar.vue'
import PreferenceView from '@/views/PreferenceView.vue'
import ExtensionPanel from '@/components/extension/ExtensionPanel.vue'

const preferences = reactive({ config: { sidebarTaskCounts: true } })
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/stores/task', () => ({
  useTaskStore: () => ({ taskCounts: { all: 8, progress: 3, failed: 1, completed: 4 } }),
}))
vi.mock('@/stores/preference', () => ({ usePreferenceStore: () => preferences }))
vi.mock('@/composables/useAppMessage', () => ({ useAppMessage: () => ({ error: vi.fn() }) }))
vi.mock('../SidebarCount.vue', () => ({
  default: { props: ['value'], template: '<span class="count">{{ value }}</span>' },
}))
vi.mock('@/components/common/MTooltip.vue', () => ({
  default: { props: ['placement', 'disabled'], template: '<slot name="trigger" />' },
}))
vi.mock('naive-ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('naive-ui')>()),
  NIcon: { template: '<span><slot /></span>' },
  NTabs: { name: 'NTabs', props: ['value'], emits: ['update:value'], template: '<div><slot /></div>' },
  NTab: { props: ['name'], template: '<span><slot /></span>' },
}))

async function setup(path: string) {
  const page = { template: '<div />' }
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/task/:status', name: 'task', component: page },
      {
        path: '/preference',
        name: 'preference',
        component: PreferenceView,
        children: [
          { path: 'general', name: 'preference-general', component: page },
          { path: 'network', name: 'preference-network', component: page },
          { path: 'advanced', name: 'preference-advanced', component: page },
        ],
      },
    ],
  })
  await router.push(path)
  await router.isReady()
  return router
}

beforeEach(() => {
  preferences.config.sidebarTaskCounts = true
})

const ExtensionHost = defineComponent({
  components: { AppSidebar, ExtensionPanel },
  setup() {
    return { show: ref(false) }
  },
  template: `
    <AppSidebar compact :extensions-open="show" @show-extensions="show = true" />
    <ExtensionPanel :show="show" @close="show = false" />
  `,
})

describe('unified navigation', () => {
  it('uses links with the current task scope and optional counts', async () => {
    const router = await setup('/task/progress')
    const wrapper = mount(AppSidebar, { global: { plugins: [router] } })
    expect(wrapper.get('a[aria-current="page"]').attributes('href')).toBe('/task/progress')
    expect(wrapper.findAll('.count').map((count) => count.text())).toEqual(['8', '3', '1', '4'])
    await wrapper.get('a[href="/task/failed"]').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.params.status).toBe('failed')
    preferences.config.sidebarTaskCounts = false
    await flushPromises()
    expect(wrapper.findAll('.count')).toHaveLength(0)
    wrapper.unmount()
  })

  it('preserves navigation and counter instances when compact mode changes', async () => {
    const router = await setup('/task/progress')
    const wrapper = mount(AppSidebar, { props: { compact: false }, global: { plugins: [router] } })
    const link = wrapper.get('a[href="/task/progress"]').element
    const counter = wrapper.get('.count').element
    await wrapper.setProps({ compact: true })
    expect(wrapper.get('a[href="/task/progress"]').element).toBe(link)
    expect(wrapper.get('.count').element).toBe(counter)
    expect(wrapper.get('a[href="/task/progress"]').attributes('aria-label')).toBe('task.scope-progress 3')
    await wrapper.setProps({ compact: false })
    expect(wrapper.get('a[href="/task/progress"]').element).toBe(link)
    wrapper.unmount()
  })

  it('keeps settings selected throughout its child routes and opens About', async () => {
    const router = await setup('/preference/network')
    const wrapper = mount(AppSidebar, { global: { plugins: [router] } })
    expect(wrapper.get('a.active').attributes('aria-label')).toBe('navigation.settings')
    expect(wrapper.get('a.active').attributes('aria-current')).toBe('page')
    await wrapper.get('button[aria-label="navigation.about"]').trigger('click')
    expect(wrapper.emitted('show-about')).toHaveLength(1)
    wrapper.unmount()
  })

  it('opens the extension panel from compact navigation and restores focus on Escape', async () => {
    const router = await setup('/task/progress')
    const wrapper = mount(ExtensionHost, {
      attachTo: document.body,
      global: { plugins: [router], stubs: { transition: false } },
    })
    const trigger = wrapper.get<HTMLButtonElement>('button[aria-label="navigation.extensions"]')
    try {
      trigger.element.focus()
      await trigger.trigger('click')
      await flushPromises()
      const panel = document.querySelector<HTMLElement>('.extension-panel')
      expect(panel?.getAttribute('role')).toBe('dialog')
      expect(panel?.getAttribute('aria-modal')).toBe('true')
      await vi.waitFor(() => expect(panel?.contains(document.activeElement)).toBe(true))
      expect(trigger.attributes('aria-expanded')).toBe('true')
      panel?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true }))
      await flushPromises()
      expect(trigger.attributes('aria-expanded')).toBe('false')
      await vi.waitFor(() => expect(document.activeElement).toBe(trigger.element))
    } finally {
      wrapper.unmount()
    }
  })

  it('respects unsaved-change navigation guards when opening extension settings', async () => {
    const router = await setup('/preference/network')
    let allowNavigation = false
    router.beforeEach(() => allowNavigation)
    const wrapper = mount(ExtensionHost, { attachTo: document.body, global: { plugins: [router] } })
    try {
      const trigger = wrapper.get('button[aria-label="navigation.extensions"]')
      for (const allowed of [false, true]) {
        allowNavigation = allowed
        await trigger.trigger('click')
        await flushPromises()
        const settings = document.querySelector<HTMLButtonElement>('.extension-settings')
        expect(settings).not.toBeNull()
        settings?.click()
        await flushPromises()
        expect(router.currentRoute.value.name).toBe(allowed ? 'preference-advanced' : 'preference-network')
        expect(trigger.attributes('aria-expanded')).toBe('false')
      }
    } finally {
      wrapper.unmount()
    }
  })

  it('respects a cancelled route leave when returning to tasks', async () => {
    const router = await setup('/preference/network')
    router.beforeEach(() => false)
    const wrapper = mount(AppSidebar, { global: { plugins: [router] } })
    await wrapper.get('a[href="/task/all"]').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.name).toBe('preference-network')
    expect(wrapper.get('a.active').attributes('aria-label')).toBe('navigation.settings')
    wrapper.unmount()
  })

  it('changes the selected settings tab only after navigation succeeds', async () => {
    const router = await setup('/preference/general')
    let allowNavigation = false
    router.beforeEach(() => allowNavigation)
    const wrapper = mount(PreferenceView, { global: { plugins: [router] } })
    const tabs = wrapper.getComponent({ name: 'NTabs' })
    tabs.vm.$emit('update:value', 'preference-network')
    await flushPromises()
    expect(tabs.props('value')).toBe('preference-general')
    allowNavigation = true
    tabs.vm.$emit('update:value', 'preference-network')
    await flushPromises()
    expect(tabs.props('value')).toBe('preference-network')
    wrapper.unmount()
  })
})
