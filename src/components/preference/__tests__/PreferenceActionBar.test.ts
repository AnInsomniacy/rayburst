import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import PreferenceActionBar from '../PreferenceActionBar.vue'

const restart = vi.fn()
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/composables/useEngineRestart', () => ({ useEngineRestart: () => ({ confirmManualRestart: restart }) }))
vi.mock('naive-ui', () => ({
  NButton: { props: ['disabled'], template: '<button :disabled="disabled"><slot /></button>' },
  NIcon: { template: '<span><slot /></span>' },
  NDropdown: { name: 'NDropdown', props: ['options'], emits: ['select'], template: '<div><slot /></div>' },
}))
beforeEach(() => {
  restart.mockReset()
})

describe('shared preference footer', () => {
  it('keeps save and discard owned by the form while rendering in the footer', async () => {
    const wrapper = mount(PreferenceActionBar, { props: { isDirty: true } })
    await flushPromises()
    expect(wrapper.find('.form-actions').exists()).toBe(true)
    const buttons = wrapper.element.querySelectorAll('button')
    buttons[0].click()
    buttons[1].click()
    expect(wrapper.emitted('save')).toHaveLength(1)
    expect(wrapper.emitted('discard')).toHaveLength(1)
    // Without changes only the engine restart remains.
    await wrapper.setProps({ isDirty: false })
    expect(wrapper.findAll('button').map((button) => button.text())).toEqual(['preferences.engine-restart-btn', ''])
    wrapper.unmount()
  })

  it('retains validation and routes compact actions through the same handlers', async () => {
    const wrapper = mount(PreferenceActionBar, { props: { isDirty: true, isValid: false } })
    await flushPromises()
    expect(wrapper.element.querySelector('button')?.disabled).toBe(true)
    const menu = wrapper.getComponent({ name: 'NDropdown' })
    menu.vm.$emit('select', 'discard')
    expect(wrapper.emitted('discard')).toHaveLength(1)
    menu.vm.$emit('select', 'restart')
    expect(restart).toHaveBeenCalledOnce()
    await wrapper.setProps({ isDirty: false })
    menu.vm.$emit('select', 'discard')
    expect(wrapper.emitted('discard')).toHaveLength(1)
    wrapper.unmount()
  })
})
