import { isEditableTarget } from '@/lib/editable-target'

describe('isEditableTarget', () => {
  it('treats inputs, text areas and content-editable elements as text fields', () => {
    const editable = document.createElement('div')
    editable.contentEditable = 'true'
    Object.defineProperty(editable, 'isContentEditable', { value: true })
    expect(isEditableTarget(document.createElement('input'))).toBe(true)
    expect(isEditableTarget(document.createElement('textarea'))).toBe(true)
    expect(isEditableTarget(editable)).toBe(true)
  })

  it('leaves other targets to the app shortcuts', () => {
    expect(isEditableTarget(document.createElement('button'))).toBeFalsy()
    expect(isEditableTarget(null)).toBe(false)
  })
})
