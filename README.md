# ToolsBag v0.2.0

精致的桌面工具盒，基于 Tauri 2 + Vue 3 构建。

---

## 功能

| 工具 | 说明 |
|------|------|
| **媒体元数据中心** | 批量导入视频/图片/音频，通过 MediaInfo / ffprobe / exiftool 提取元数据，支持整理命名、视频体检与报告导出 |
| **PDF合成（原样）** | 无损合成图片为 PDF，保留原始 JPEG 字节，支持 CMYK 转换与 EXIF 方向校正 |
| **文件遍历提取** | 多线程高速文件复制，按格式过滤与分类，支持媒体预览 |
| **码本** | Argon2id + AES-256-GCM 加密的密码保险箱，支持 WebDAV 多设备同步、VaultX V2 导入导出、设备管理与回收站 |
| **图片合成PDF** | 高确定性图片合并 PDF，支持 A4/A3/B5/iPad Pro 等页面尺寸、边距控制、JPEG 质量调节 |

## 技术栈

- **前端**：Vue 3 + TypeScript + Element Plus + ECharts
- **后端**：Rust + Tauri 2
- **构建**：Vite 7
- **加密**：Argon2id / AES-256-GCM / HMAC-SHA256 / Ed25519
- **并行处理**：Rayon

## 开发

```bash
# 安装依赖
npm install

# 启动前端开发服务器
npm run dev

# 前端类型检查
npm run typecheck

# Rust 后端检查
npm run rust:check

# Rust 单元测试
npm run rust:test

# Rust fmt / clippy / 前后端综合门禁
npm run quality

# 启动桌面应用（开发模式）
npm run tauri:dev

# 构建前端生产资源
npm run build

# 构建生产版本
npm run tauri:build
```

### 为什么同时有 npm 和 Cargo

ToolsBag 是 Tauri 应用：Vue/Vite 前端由 npm 生态管理，Rust 后端由 Cargo 管理。`npm run dev` 只启动 Web UI；`npm run tauri:dev` 会通过本地 Tauri CLI 同时拉起前端和 Rust 桌面壳；纯 Rust 后端检查可以直接用 `cargo check --manifest-path src-tauri/Cargo.toml`。

## 环境要求

- Node.js 18+
- Rust 1.75+
- Tauri CLI 2.x
- 可选：MediaInfo 动态库。Windows 开发机可把 `MediaInfo.dll` 放在 `src-tauri/MediaInfo.dll` 或系统 PATH；该文件不再入库，打包时请按目标平台提供对应 `.dll` / `.dylib` / `.so`。

## 项目结构

```
toolsbag1/
├── core/               # Vue 公共层（App、hooks、API、组件）
├── tools/              # 各工具前端模块
│   ├── media-batch/    # 媒体元数据中心
│   ├── image-batch/    # PDF 合成（原样）
│   ├── image-to-pdf/   # 图片合成 PDF
│   ├── file-traverse/  # 文件遍历提取
│   └── codebook/       # 码本
├── config/             # Vite + TypeScript 配置
├── public/             # 静态资源
├── src-tauri/          # Rust 后端
│   ├── src/
│   │   ├── cmd/        # Tauri 命令层
│   │   ├── models/     # 数据结构
│   │   ├── tools/      # 核心工具逻辑
│   │   ├── codebook.rs # 加密保险箱引擎
│   │   ├── webdav.rs   # WebDAV 同步
│   │   └── crypto.rs   # 统一加密层
│   └── tauri.conf.json
└── index.html
```

## 许可证

MIT

## 安全说明

- 码本数据使用 Argon2id + AES-256-GCM + HMAC-SHA256 保护。
- WebDAV 同步配置包含服务端凭据；新版保存时会在保险库解锁状态下加密落盘，旧版明文配置只会在解锁后迁移为加密格式。
- WebDAV 远端内容使用保险库主密钥派生同步密钥后端到端加密；HTTP WebDAV 仅允许 localhost / 127.0.0.1 调试，生产地址必须使用 HTTPS。
- Tauri 前端不再暴露 `fs:read-all` / `fs:write-all` / `shell:default` 权限；文件写入、重命名和扫描统一经后端命令做范围校验。
- `src-tauri/MediaInfo.dll`、`*.dll`、`*.dylib`、`*.so` 已忽略，`.gitattributes` 同时为必须入库的二进制预留 Git LFS 规则。
