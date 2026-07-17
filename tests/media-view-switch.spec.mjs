import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const mediaToolSource = await readFile(
  new URL('../tools/media-batch/src/index.vue', import.meta.url),
  'utf8',
)

test('media tool keeps both large views mounted while switching', () => {
  assert.match(mediaToolSource, /v-show="viewMode === 'batch'"/)
  assert.match(mediaToolSource, /v-show="viewMode === 'exhibition'"/)
  assert.doesNotMatch(mediaToolSource, /v-if="viewMode === 'batch'"/)
  assert.doesNotMatch(mediaToolSource, /<KeepAlive>/)
})
