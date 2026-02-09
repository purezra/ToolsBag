# ToolsBag

[中文](#中文) | [English](#english) | [日本語](#日本語)

---

## 中文

精致的桌面工具盒，基于 Tauri + Vue 3 构建。

### 功能

- **媒体批处理** - 批量提取视频/图片信息，重命名与可视化分析
- **PDF合成（原样）** - 无损合成图片为PDF，支持批量处理
- **文件遍历提取** - 高速复制、按格式分类与过滤
- **码本** - 密码生成器与账号资产管理，安全存储
- **图片合成PDF** - 高确定性图片合并PDF，支持边距、多尺寸、预览

### 技术栈

- 前端：Vue 3 + TypeScript + Element Plus + ECharts
- 后端：Rust + Tauri
- 构建：Vite

### 开发

```bash
# 安装依赖
npm install

# 启动开发服务器
npm run dev

# 启动桌面应用（开发模式）
npm run tauri:dev

# 构建生产版本
npm run tauri:build
```

### 环境要求

- Node.js 18+
- Rust 1.60+
- Tauri CLI

### 许可证

MIT

---

## English

A refined desktop toolbox built with Tauri + Vue 3.

### Features

- **Media Batch Processing** - Batch extract video/image info, rename and visualize analysis
- **PDF Merge (Lossless)** - Lossless image-to-PDF conversion with batch support
- **File Traverse** - High-speed copy, filter and organize by format
- **Codebook** - Password generator and account asset manager with secure storage
- **Image to PDF** - Deterministic image-to-PDF with margins, multi-size support and preview

### Tech Stack

- Frontend: Vue 3 + TypeScript + Element Plus + ECharts
- Backend: Rust + Tauri
- Build: Vite

### Development

```bash
# Install dependencies
npm install

# Start dev server
npm run dev

# Launch desktop app (dev mode)
npm run tauri:dev

# Build for production
npm run tauri:build
```

### Requirements

- Node.js 18+
- Rust 1.60+
- Tauri CLI

### License

MIT

---

## 日本語

Tauri + Vue 3 で構築された洗練されたデスクトップツールボックス。

### 機能

- **メディア一括処理** - 動画/画像情報の一括抽出、リネームと可視化分析
- **PDF合成（無損失）** - 画像を無損失でPDFに変換、バッチ処理対応
- **ファイル走査** - 高速コピー、フォーマット別フィルタリングと整理
- **コードブック** - パスワード生成とアカウント資産管理、安全な保存
- **画像からPDF** - 余白、複数サイズ、プレビュー対応の高精度PDF変換

### 技術スタック

- フロントエンド：Vue 3 + TypeScript + Element Plus + ECharts
- バックエンド：Rust + Tauri
- ビルド：Vite

### 開発

```bash
# 依存関係をインストール
npm install

# 開発サーバーを起動
npm run dev

# デスクトップアプリを起動（開発モード）
npm run tauri:dev

# 本番用にビルド
npm run tauri:build
```

### 必要環境

- Node.js 18+
- Rust 1.60+
- Tauri CLI

### ライセンス

MIT
