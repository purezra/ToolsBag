# ToolsBag v0.2.0

精致的桌面工具盒，基于 Tauri 2 + Vue 3 构建。

---

## 功能

| 工具 | 说明 |
|------|------|
| **媒体批处理** | 批量导入视频/图片/音频，通过 MediaInfo / ffprobe / exiftool 提取元数据，支持视频详情展示与 EXIF 分析 |
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

# 启动桌面应用（开发模式）
npm run tauri:dev

# 构建生产版本
npm run tauri:build
```

## 环境要求

- Node.js 18+
- Rust 1.75+
- Tauri CLI 2.x

## 项目结构

```
toolsbag1/
├── core/               # Vue 公共层（App、hooks、API、组件）
├── tools/              # 各工具前端模块
│   ├── media-batch/    # 媒体批处理
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
