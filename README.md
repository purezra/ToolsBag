# ToolsBag

基于 Tauri 2 + Vue 3 的桌面工具盒，Rust 后端驱动，Vite 构建前端。

## 功能

| 工具 | 说明 |
|------|------|
| **媒体探针** | 批量导入视频/图片/音频，提取元数据，支持整理命名、视频体检与报告导出 |
| **图片转 PDF** | 图片合并 PDF 的统一入口：支持单份、按数量分批（100/200/300/全部）、每子文件夹一份；原样无损（JPEG 直传 + Flate 无损）、智能压缩与便携体积三档；A4/A3/B5 幅面、边距、多页 TIFF、ICC 保真/兼容与页面预览 |
| **图片打包（EPUB）** | 把文件夹图片打包为 EPUB 电子书，支持递归扫描、输出目录与 iPad Pro/iPad Mini/A4/B5 页面尺寸；原图片批处理的 PDF 合成已整合到「图片转 PDF」 |
| **图片压缩** | 批量图片压缩，支持格式转换与质量调节 |
| **图片工具** | 图片格式转换、缩放、裁剪等常用操作 |
| **PDF 合并** | 把多份 PDF、图片与 DOC/DOCX 按顺序合并为一份 PDF：支持按页数/修改时间升降序、自由拖拽排序；Word 文档可指定参与合并的页码（如 1,5,7 或 1-3,5）；完成后可复制 PDF、打开所在文件夹或继续合并 |
| **文件收割** | 多线程高速文件复制，按格式过滤与分类，支持媒体预览 |
| **密码册** | Argon2id + AES-256-GCM 加密的密码保险箱，支持 WebDAV 多设备同步 |
| **图像套件 · 拼版印刷** | 把文件夹里的图片密集拼贴成多页 PDF（切分树 + 模拟退火），也可整页导出 WebP/AVIF；支持 A4/A3、边距/间距、逐页张数与 DPI 上限。拼版印刷现为「图像套件」的第四个标签页 |

## 技术栈

- **前端**：Vue 3 + TypeScript + Element Plus + ECharts
- **后端**：Rust + Tauri 2
- **构建**：Vite 7
- **加密**：Argon2id / AES-256-GCM / HMAC-SHA256 / Ed25519

## 界面与主题

亮色 / 暗色主题基于 Material 3 动态取色（种子色 `#007AFF`，ColorStyle.vivid）生成完整的 role token，
视觉风格对齐 FlClash：无边框卡片、分层表面（page / card / block / inset）、连续圆角、iOS 分组设置列表
与填充式分段控件。所有颜色、圆角、阴影都通过 `core/assets/styles/skin-minimal.css` 中的 CSS 变量下发，
工具模块只引用 `var(--*)` 令牌，新增组件时请沿用同一套变量以保持两种模式一致。

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
- Windows 开发需要两个本地二进制（编译期嵌入，见下）：
  - 仓库根目录的 `MediaInfo.dll`
  - `src-tauri/bin/libjxl/cjxl.exe`

两者体积较大且许可证各异，未纳入版本库，新克隆的仓库需手动准备，否则 `cargo check` / `cargo build` 会在 `build.rs` 阶段失败并给出下载地址。

## 打包与分发

`npm run tauri:build` 会把前端产物、`MediaInfo.dll` 和 `bin/libjxl/cjxl.exe`
一起打进单个可执行文件，产物 `ToolsBag.exe` **双击即可运行**，不需要额外安装或拷贝 DLL。
这两个二进制会由 `src-tauri/build.rs` 在编译期先做 deflate 压缩再 `include_bytes!`
（8.3MB → 3.5MB、4.7MB → 2.0MB），运行时首次使用时解压到 `%TEMP%`。

### 便携版自更新

设置 →「应用更新」支持软件内检查、下载和重启更新，流程与 magpie 类似：

1. 读取 GitHub Releases 最新版本（`purezra/ToolsBag`，资产名固定为 `ToolsBag-Portable.exe`）；
2. 在应用内下载到 `%TEMP%`，显示进度；
3. 下载完成后出现「重启更新」，点击后由 `self-replace` 替换当前 exe 并重启。

发布新版本时：提升 `package.json` / `src-tauri/Cargo.toml` / `tauri.conf.json` 的版本号，
构建 `app.exe` 并重命名为 `ToolsBag-Portable.exe`，创建 GitHub Release（tag 形如 `v0.2.1`），
把 exe 作为资产上传即可。若 GitHub 返回资产 `digest`，下载后会校验 SHA-256。

### 准备本地二进制

```bash
# 1) MediaInfo.dll —— 放到仓库根目录（与 package.json 同级）
#    来源：https://mediaarea.net/en/MediaInfo/Download/Windows（选 64 位 DLL 包）

# 2) cjxl.exe —— 放到 src-tauri/bin/libjxl/
#    来源：https://github.com/libjxl/libjxl/releases 的 jxl-x64-windows-static.zip
```

`src-tauri/build.rs` 会在开始编译前检查这两个文件，缺失时直接报错并打印上述下载地址，
避免只看到一句编译器层面的 `couldn't read file`。

## 项目结构

```
toolsbag1/
├── core/               # Vue 公共层（App、hooks、API、组件）
├── tools/              # 各工具前端模块
│   ├── media-batch/    # 媒体探针
│   ├── image-batch/    # 图片打包（EPUB）
│   ├── image-to-pdf/   # 图片转 PDF（统一入口）
│   ├── image-compress/ # 图片压缩
│   ├── image-tools/    # 图片工具
│   ├── pdf-merge/      # PDF 合并（PDF/图片/Word 混合、排序与选页）
│   ├── file-traverse/  # 文件收割
│   ├── codebook/       # 密码册
│   └── tilepress/      # 拼版印刷（UI 与样式整体收敛到 .tilepress-root）
├── config/             # Vite + TypeScript 配置
├── public/             # 静态资源
├── build/              # 前端生产构建产物
├── docs/               # 项目文档
├── exe/                # 打包输出目录
├── src-tauri/          # Rust 后端
│   ├── bin/libjxl/     # cjxl.exe（本地准备，不入库）
│   ├── engines/tilepress/ # 拼版引擎（独立 path crate，保持原项目代码）
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
