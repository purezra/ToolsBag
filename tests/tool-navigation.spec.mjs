import assert from 'node:assert/strict'
import { createReadStream, existsSync, statSync } from 'node:fs'
import { createServer } from 'node:http'
import { extname, join, normalize } from 'node:path'
import test from 'node:test'
import { chromium } from 'playwright'

const buildDir = join(process.cwd(), 'build')
const mime = {
  '.css': 'text/css',
  '.html': 'text/html',
  '.js': 'text/javascript',
  '.webp': 'image/webp',
}

test('all launcher cards open their tool page', async () => {
  assert.ok(existsSync(join(buildDir, 'index.html')), 'run npm run build first')

  const server = createServer((req, res) => {
    const requested = normalize(decodeURIComponent(req.url?.split('?')[0] || '/')).replace(/^[/\\]+/, '')
    let file = join(buildDir, requested || 'index.html')
    if (!existsSync(file) || statSync(file).isDirectory()) file = join(buildDir, 'index.html')
    res.setHeader('Content-Type', mime[extname(file)] || 'application/octet-stream')
    createReadStream(file).pipe(res)
  })
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))
  const port = server.address().port
  const browser = await chromium.launch()

  try {
    const page = await browser.newPage()
    await page.goto(`http://127.0.0.1:${port}`)

    const tools = [
      ['媒体探针', '.media-tool'],
      ['图像套件', '.image-tools-shell'],
      ['文件收割', '.file-traverse'],
      ['密码册', '.codebook-tool'],
    ]

    for (const [name, selector] of tools) {
      await page.locator('.tool-card', { hasText: name }).click()
      await page.locator('.topbar-page-title__name', { hasText: name }).waitFor()
      await page.locator(selector).waitFor()
      await page.locator('.nav-item[title="工具中心"]').click()
      await page.locator('.tool-home').waitFor()
    }

    // Imposition is now the fourth tab inside 图像套件, not a standalone tool.
    await page.locator('.tool-card', { hasText: '图像套件' }).click()
    await page.locator('.seg-btn', { hasText: '拼版印刷' }).click()
    await page.locator('.tilepress-root').waitFor()
  } finally {
    await browser.close()
    await new Promise((resolve) => server.close(resolve))
  }
})

test('image-to-pdf page scrolls instead of clipping the preview', async () => {
  assert.ok(existsSync(join(buildDir, 'index.html')), 'run npm run build first')

  const server = createServer((req, res) => {
    const requested = normalize(decodeURIComponent(req.url?.split('?')[0] || '/')).replace(/^[/\\]+/, '')
    let file = join(buildDir, requested || 'index.html')
    if (!existsSync(file) || statSync(file).isDirectory()) file = join(buildDir, 'index.html')
    res.setHeader('Content-Type', mime[extname(file)] || 'application/octet-stream')
    createReadStream(file).pipe(res)
  })
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))
  const port = server.address().port
  const browser = await chromium.launch()

  try {
    const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } })
    await page.goto(`http://127.0.0.1:${port}`)
    await page.locator('.tool-card', { hasText: '图像套件' }).click()
    await page.locator('.seg-btn', { hasText: '图片转 PDF' }).click()
    await page.locator('.img2pdf').waitFor()

    const result = await page.evaluate(async () => {
      const root = document.querySelector('.img2pdf')
      // Simulate expanded settings + analysis cards pushing the preview down.
      const filler = document.createElement('div')
      filler.style.cssText = 'height: 1400px; flex: 0 0 auto;'
      root.insertBefore(filler, root.querySelector('.preview-panel'))
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
      const canScroll = root.scrollHeight > root.clientHeight
      root.scrollTop = root.scrollHeight
      await new Promise((resolve) => requestAnimationFrame(resolve))
      return {
        overflowY: getComputedStyle(root).overflowY,
        canScroll,
        scrollTop: root.scrollTop,
        previewHeight: root.querySelector('.preview-panel').clientHeight,
      }
    })

    assert.equal(result.overflowY, 'auto')
    assert.ok(result.canScroll, 'page should be scrollable when content exceeds the viewport')
    assert.ok(result.scrollTop > 0, 'page should scroll down to the preview')
    assert.ok(result.previewHeight >= 420, 'preview keeps a usable height')
  } finally {
    await browser.close()
    await new Promise((resolve) => server.close(resolve))
  }
})

test('application shell never creates a window-level scrollbar', async () => {
  assert.ok(existsSync(join(buildDir, 'index.html')), 'run npm run build first')

  const server = createServer((req, res) => {
    const requested = normalize(decodeURIComponent(req.url?.split('?')[0] || '/')).replace(/^[/\\]+/, '')
    let file = join(buildDir, requested || 'index.html')
    if (!existsSync(file) || statSync(file).isDirectory()) file = join(buildDir, 'index.html')
    res.setHeader('Content-Type', mime[extname(file)] || 'application/octet-stream')
    createReadStream(file).pipe(res)
  })
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))
  const port = server.address().port
  const browser = await chromium.launch()

  try {
    for (const viewport of [{ width: 960, height: 640 }, { width: 1440, height: 900 }]) {
      const page = await browser.newPage({ viewport })
      await page.goto(`http://127.0.0.1:${port}`)
      const metrics = await page.evaluate(() => ({
        bodyWidth: document.body.scrollWidth,
        bodyHeight: document.body.scrollHeight,
        rootWidth: document.documentElement.scrollWidth,
        rootHeight: document.documentElement.scrollHeight,
        viewportWidth: window.innerWidth,
        viewportHeight: window.innerHeight,
      }))
      assert.deepEqual(metrics, {
        bodyWidth: viewport.width,
        bodyHeight: viewport.height,
        rootWidth: viewport.width,
        rootHeight: viewport.height,
        viewportWidth: viewport.width,
        viewportHeight: viewport.height,
      })
      await page.close()
    }
  } finally {
    await browser.close()
    await new Promise((resolve) => server.close(resolve))
  }
})
