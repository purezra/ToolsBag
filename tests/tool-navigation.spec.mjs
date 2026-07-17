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

test('all four launcher cards open their tool page', async () => {
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
      ['图片工坊', '.image-tools-shell'],
      ['文件收割', '.file-traverse'],
      ['密码册', '.codebook-tool'],
    ]

    for (const [name, selector] of tools) {
      await page.locator('.tool-card', { hasText: name }).click()
      await page.locator('.topbar-page-title__name', { hasText: name }).waitFor()
      await page.locator(selector).waitFor()
      await page.locator('.topbar-brand').click()
      await page.locator('.tool-home').waitFor()
    }
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
