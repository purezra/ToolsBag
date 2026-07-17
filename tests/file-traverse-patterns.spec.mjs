import assert from 'node:assert/strict'
import test from 'node:test'

test('empty format selection is distinguishable from selecting every format', async () => {
  const pathUtils = await import('../tools/file-traverse/src/utils/path.ts')
  assert.equal(typeof pathUtils.hasSelectedFormats, 'function')
  assert.equal(pathUtils.hasSelectedFormats({ image: [], video: [] }), false)
  assert.equal(pathUtils.hasSelectedFormats({ image: ['png'], video: [] }), true)
})
