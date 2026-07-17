# ToolsBag

基于 Tauri 2 + Vue 3 的桌面工具盒，Rust 后端驱动，Vite 构建前端。

## 功能

| 工具 | 说明 |
|------|------|
| **媒体探针** | 批量导入视频/图片/音频，提取元数据，支持整理命名、视频体检与报告导出 |
| **PDF合成（原样）** | 无损合成图片为 PDF，保留原始 JPEG 字节，支持 CMYK 转换与 EXIF 方向校正 |
| **图片合成 PDF** | 高确定性图片合并 PDF，支持 A4/A3/B5 等页面尺寸、边距控制、JPEG 质量调节 |
| **图片压缩** | 批量图片压缩，支持格式转换与质量调节 |
| **图片工具** | 图片格式转换、缩放、裁剪等常用操作 |
| **文件收割** | 多线程高速文件复制，按格式过滤与分类，支持媒体预览 |
| **密码册** | Argon2id + AES-256-GCM 加密的密码保险箱，支持 WebDAV 多设备同步 |

## 技术栈

- **前端**：Vue 3 + TypeScript + Element Plus + ECharts
- **后端**：Rust + Tauri 2
- **构建**：Vite 7
- **加密**：Argon2id / AES-256-GCM / HMAC-SHA256 / Ed25519

## 开发

```bash
# 安装依赖
npm install

# 仅启动前端开发服务器（浏览器预览 UI，IPC 调用不可用）
npm run dev

# 启动桌面应用（开发模式，前后端全功能可用）
npm run tauri:dev

# 前端类型检查
npm run typecheck

# Rust 后端检查
npm run rust:check

# Rust 单元测试
npm run rust:test

# 打包生产版本（产物输出到 exe/）
npm run tauri:build
```

## 启动方式说明

| 命令 | 用途 | 说明 |
|------|------|------|
| `npm run dev` | 纯前端预览 | 浏览器 http://localhost:5173，用于 UI 交互/样式调试 |
| `npm run tauri:dev` | 全功能联调 | 启动桌面窗口，前后端 IPC 全通 |
| `npm run tauri:build` | 打包 | 输出到 `exe/` 目录（通过 `TAURI_BUNDLE_DIR` 指定） |

## 环境要求

- Node.js 18+
- Rust 1.77+
- Tauri CLI 2.x
- Windows 开发需将 `MediaInfo.dll` 放在项目根目录

## 项目结构

```
toolsbag1/
├── core/               # Vue 公共层（App、hooks、API、组件）
├── tools/              # 各工具前端模块
│   ├── media-batch/    # 媒体探针
│   ├── image-batch/    # PDF 合成（原样）
│   ├── image-to-pdf/   # 图片合成 PDF
│   ├── image-compress/ # 图片压缩
│   ├── image-tools/    # 图片工具
│   ├── file-traverse/  # 文件收割
│   └── codebook/       # 密码册
├── config/             # Vite + TypeScript 配置
├── public/             # 静态资源
├── build/              # 前端生产构建产物
├── docs/               # 项目文档
├── exe/                # 打包输出目录
├── src-tauri/          # Rust 后端
│   ├── src/
│   │   ├── cmd/        # Tauri 命令层
│   │   ├── models/     # 数据结构
│   │   ├── tools/      # 核心工具逻辑
│   │   ├── codebook.rs
│   │   ├── webdav.rs
│   │   └── crypto.rs
│   └── tauri.conf.json
├── index.html
├── MediaInfo.dll
└── package.json
```

## 许可证

MIT
