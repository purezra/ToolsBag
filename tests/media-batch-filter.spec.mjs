import assert from 'node:assert/strict'
import test from 'node:test'

import * as mediaUtils from '../tools/media-batch/src/utils/media-utils.ts'

const evaluate = mediaUtils.evaluateVideoFilter
const splitMediaFormatCounts = mediaUtils.splitMediaFormatCounts
const paginateRows = mediaUtils.paginateRows
const now = Date.UTC(2026, 6, 9, 12)

const filters = (overrides = {}) => ({
  formats: [],
  durationMode: 'all',
  durationMin: 0,
  durationMax: 10,
  durationUnit: 'minute',
  recentDays: null,
  ...overrides,
})

const video = (overrides = {}) => ({
  name: 'clip.mp4',
  durationSec: 90,
  arrivalTimeMs: now - 24 * 60 * 60 * 1000,
  ...overrides,
})

test('combines filter dimensions with AND and formats with OR', () => {
  assert.equal(typeof evaluate, 'function')
  assert.equal(evaluate(video({ name: 'clip.MKV' }), filters({
    formats: ['mp4', 'mkv'],
    durationMode: 'between',
    durationMin: 1,
    durationMax: 2,
    recentDays: 3,
  }), now), 'match')
  assert.equal(evaluate(video({ name: 'clip.mov' }), filters({
    formats: ['mp4', 'mkv'],
    durationMode: 'between',
    durationMin: 1,
    durationMax: 2,
    recentDays: 3,
  }), now), 'no-match')
})

test('includes duration and recent-day boundaries', () => {
  assert.equal(typeof evaluate, 'function')
  const range = filters({
    durationMode: 'between',
    durationMin: 1,
    durationMax: 2,
    recentDays: 7,
  })
  assert.equal(evaluate(video({
    durationSec: 60,
    arrivalTimeMs: now - 7 * 24 * 60 * 60 * 1000,
  }), range, now), 'match')
  assert.equal(evaluate(video({ durationSec: 120 }), range, now), 'match')
})

test('supports one-decimal thresholds in seconds, minutes, and hours', () => {
  assert.equal(typeof evaluate, 'function')
  assert.equal(evaluate(video({ durationSec: 90 }), filters({
    durationMode: 'at-most',
    durationMax: 1.5,
    durationUnit: 'minute',
  }), now), 'match')
  assert.equal(evaluate(video({ durationSec: 5400 }), filters({
    durationMode: 'at-least',
    durationMin: 1.5,
    durationUnit: 'hour',
  }), now), 'match')
})

test('marks missing required metadata separately', () => {
  assert.equal(typeof evaluate, 'function')
  assert.equal(evaluate(video({ durationSec: undefined }), filters({
    durationMode: 'at-most',
  }), now), 'missing')
  assert.equal(evaluate(video({ arrivalTimeMs: undefined }), filters({
    recentDays: 1,
  }), now), 'missing')
  assert.equal(evaluate(video({ durationSec: null }), filters({
    durationMode: 'at-most',
  }), now), 'missing')
  assert.equal(evaluate(video({ arrivalTimeMs: null }), filters({
    recentDays: 1,
  }), now), 'missing')
})

test('splits import formats into video and image groups', () => {
  assert.equal(typeof splitMediaFormatCounts, 'function')
  assert.deepEqual(splitMediaFormatCounts([
    { ext: 'jpg', count: 2 },
    { ext: 'mp4', count: 18 },
    { ext: 'mkv', count: 3 },
  ]), {
    videoFormats: [
      { ext: 'mp4', count: 18 },
      { ext: 'mkv', count: 3 },
    ],
    imageFormats: [
      { ext: 'jpg', count: 2 },
    ],
  })
})

test('paginates large media lists and clamps stale page numbers', () => {
  assert.equal(typeof paginateRows, 'function')
  const rows = Array.from({ length: 121 }, (_, index) => index + 1)

  assert.deepEqual(paginateRows(rows, 2, 50), {
    page: 2,
    pageCount: 3,
    rows: rows.slice(50, 100),
  })
  assert.deepEqual(paginateRows(rows, 99, 50), {
    page: 3,
    pageCount: 3,
    rows: rows.slice(100),
  })
  assert.deepEqual(paginateRows([], 5, 50), {
    page: 1,
    pageCount: 1,
    rows: [],
  })
})
