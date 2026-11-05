# Tilepress

**Tilepress —— 把一个文件夹的图片，密集拼贴成多页 PDF。**

`tilepress` = **tile**（瓷砖/铺满）+ **press**（印刷机）。名字说的就是它干的事：把一堆尺寸不一的图当成瓷砖，按最小白边铺满每一页纸，然后像印刷机一样吐出 PDF。

不是相册，不是幻灯片，是**拼版工具**。

---

## 它解决什么

手工把 500 张图排进 PDF，要么留一堆白边，要么一张一张拖。Tilepress 只做一件事：**给定纸张、边距、间距，把图片排得最满**。

- 排版引擎：二叉切分树（slicing tree）+ 模拟退火 + 全局挑图换位 + 行式网格种子，目标函数 = **整页白边面积最小**
- 真实照片实测平均填充率 **96%+**（等大网格只有 86% 左右）
- 1500 张混合长宽比图片，A4 竖版 64 张/页 → 24 页，扫描 0.3s + 排版 1.7s + 写 PDF 0.3s

## 图片导出与清晰度

参数页左侧集中设置纸张、版式、页边距、间距和分页；右侧选择 PDF / WebP / AVIF 及输出质量。排序、过滤和图片统计可展开查看。

- 经典网格保留完整内容与原始比例，所有图片都限制在指定页边距内；自由拼贴可另选铺满裁边或贴合。
- 图片导出默认「最高保真 + 保留原图像素」：按每页中各张原图所需的像素密度自动决定整页分辨率，避免固定 300 DPI 缩小细字。也可明确选择 150–600 DPI。
- WebP 最高保真使用真正的无损编码，保存合成后的全部 RGB 像素；AVIF 使用质量 100、完整 4:4:4 色彩采样及 RGB 编码，采用均衡搜索策略减少等待，仍属于有损编码。整页拼图本身仍会按版面重采样，不等于直接复制原文件字节。
- PDF 原图无损通常只需封装已有图片数据；图片输出需要合成并重新编码整页，因此耗时不能与 PDF 的字节直塞等同。默认构建启用 AVIF SIMD CPU 优化，保持质量参数不变。
- WebP 无损使用速度优先策略：降低压缩搜索开销，解码像素与原来的无损策略一致，代价是输出文件更大，不降低分辨率或使用近无损编码。
- AVIF 的均衡提速保持分辨率、质量档和色彩采样不变；与原来的慢速搜索相比，文件可能变大，像素结果可能有极小差异。
- 图片完成页提供打开图片文件夹、复制所有图、返回预览、调整参数、继续拼图。复制的是本次导出的全部图片文件，可粘贴到 Windows 文件夹；继续拼图会重新选择输入文件夹。
- 自动分辨率超过 WebP 单边 16383 像素上限或整页合成像素预算时会明确报错，可减少每页张数或指定 DPI，不会悄悄降低清晰度。

回归检查：`cargo test`；前端检查：`pnpm build`；便携 EXE：`pnpm tauri build --no-bundle`。

## 功能

| | |
|---|---|
| 幅面 | A4 / A3，竖版 / 横版 |
| 排版 | 拼贴（切分树，铺满）、等大网格（经典模式，不裁剪） |
| 模式 | 铺满 cover（裁切填满，无缝）/ 贴合 fit（不裁切，留细白边） |
| 分页 | 固定每页张数，或指定总页数反算；也可按真实长宽比分布用 DP 逐页规划（各页张数可不等） |
| 输出质量 | 原图无损（JPEG DCTDecode / PNG Flate 原始字节直塞，零重编码）、q92 / q85 / q75 |
| DPI 上限 | 150 / 200 / 300 / 450 / 不限，按格子物理尺寸降采样（省体积杀器） |
| 预览 | 后端按 3 px/mm 合成整页预览图，滚动 / 单页 / 全屏，与最终 PDF 同一份坐标数据 |
| 大文件 | PDF 流式写入，内存峰值与图片总数无关；2000 张 / 8GB 峰值 < 300MB |
| 便携 | 单文件 exe，配置和缓存写在 exe 同级，不写注册表，拷到新电脑双击即用 |

## 技术栈

- **核心引擎**：纯 Rust（`src/`）—— 排版、图片头解析、PDF 写入、预览合成
  - `layout.rs` 切分树 + 模拟退火排版
  - `imgprobe.rs` 只读文件头拿尺寸，不解码像素
  - `pdf.rs` 流式 PDF 写入器（原始字节直塞）
  - `encode.rs` 重编码 / 降采样（zune-jpeg + fast_image_resize + jpeg-encoder）
  - `preview.rs` 整页预览合成与缓存
- **GUI**：Tauri 2 + Vue 3 + TypeScript + Vite（`ui/`、`src-tauri/`）
- **无需大型图像框架**：Rust 核心 + 前端，编解码依赖含 C/汇编实现；无需 libvips / OpenCV / CUDA

## 构建

```bash
pnpm install
pnpm tauri build          # → src-tauri/target/release/tilepress.exe + NSIS 安装包
```

只编核心引擎 / CLI：

```bash
cargo build --release     # → target/release/tilepress.exe
```

## CLI 用法

```bash
tilepress --dir <图片文件夹> --page a4p|a4l|a3p|a3l --n 64 \
          --margin 10 --gutter 2 --mode cover|fit \
          --quality lossless|q92|q85|q75 --dpi 300 \
          --pdf out.pdf [--pages K] [--plan 62,64,60]
```

## 从源码构建

构建最高性能版本需安装 [NASM](https://www.nasm.us/) 2.14.02 或更新版本，并把 `nasm.exe` 所在目录加入构建环境的 PATH。它仅在编译时使用；生成的便携 EXE 不需要安装 NASM，运行时会按 CPU 能力选择优化代码。只编 CLI 且缺少汇编器时，可用 `cargo build --release --no-default-features`，但 AVIF 会更慢。

构建环境（其余依赖由构建脚本自动拉取）：

| 依赖 | 版本 | 说明 |
|---|---|---|
| Rust | 1.77+ (stable, MSVC) | `rustup target add x86_64-pc-windows-msvc` |
| Node.js | 20+ | 带 npm |
| pnpm | 9+ | `npm i -g pnpm` |
| NASM | 2.14.02+ | 加入 PATH，启用 AVIF CPU 优化；仅构建时使用 |

```bash
git clone git@gitee.com:pig_tree/tilepress.git
cd tilepress

pnpm install            # 前端依赖
pnpm tauri build        # 产出 GUI 便携 exe + NSIS 安装包

# 产物：
#   src-tauri/target/release/tilepress.exe                     ← 单体便携版
#   src-tauri/target/release/bundle/nsis/Tilepress_0.1.0_x64-setup.exe
```

只要命令行版（不需要 Node / WebView2）：

```bash
cargo build --release   # → target/release/tilepress.exe
```

运行环境：Windows 10/11。GUI 版依赖系统自带的 WebView2（Win11 预装，Win10 缺了程序会提示下载）。

## 项目结构

```
src/            核心引擎（纯 Rust，无 GUI 依赖）
  layout.rs       排版：行式网格种子、等面积分行、切分树 + 退火
  lib.rs          任务编排、逐页规划 DP、推荐算法、流式导出
  pdf.rs          PDF 写入（JPEG DCTDecode / PNG Flate 原始字节直塞）
  imgprobe.rs     只读文件头解析尺寸（4KB→512KB 渐进）
  encode.rs       质量档 / DPI 上限 / WebP 转码
  preview.rs      整页预览合成与缓存
  main.rs         命令行入口
src-tauri/      Tauri 2 外壳（窗口、命令、便携路径、暗色 DWM 边框）
ui/             Vue 3 + TypeScript 界面（Dracula 配色）
```

## 许可

个人项目，未指定许可证。
