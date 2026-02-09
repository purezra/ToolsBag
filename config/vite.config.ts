import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import path from 'path'

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
      input: path.resolve(__dirname, '../index.html')
    }
  },
  plugins: [vue()]
})
