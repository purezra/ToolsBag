import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import path from 'path'
import AutoImport from 'unplugin-auto-import/vite'
import Components from 'unplugin-vue-components/vite'
import { ElementPlusResolver } from 'unplugin-vue-components/resolvers'

export default defineConfig({
  root: path.resolve(__dirname, '../'),
  resolve: {
    alias: {
      '@core': path.resolve(__dirname, '../core'),
      '@codebook': path.resolve(__dirname, '../tools/codebook/src'),
      '@file-traverse': path.resolve(__dirname, '../tools/file-traverse/src'),
      '@image-batch': path.resolve(__dirname, '../tools/image-batch/src'),
      '@media-batch': path.resolve(__dirname, '../tools/media-batch/src'),
      '@image-to-pdf': path.resolve(__dirname, '../tools/image-to-pdf/src')
    }
  },
  build: {
    outDir: path.resolve(__dirname, '../build'),
    rollupOptions: {
      input: path.resolve(__dirname, '../index.html'),
      output: {
        manualChunks: {
          'vue-vendor': ['vue', 'element-plus'],
          'echarts-vendor': ['echarts']
        }
      }
    },
    chunkSizeWarningLimit: 1000
  },
  server: {
    port: 5173,
    strictPort: false
  },
  plugins: [
    vue(),
    AutoImport({
      resolvers: [ElementPlusResolver()],
      dts: path.resolve(__dirname, 'auto-imports.d.ts')
    }),
    Components({
      resolvers: [ElementPlusResolver()],
      dts: path.resolve(__dirname, 'components.d.ts')
    })
  ]
})
