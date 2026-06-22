import { computed, inject, provide, ref, watch } from 'vue'

export type ThemeMode = 'light' | 'dark'
export type ThemeSkin = 'modern'
export type Appearance = 'light' | 'dark' | 'system'
export type Locale = 'zh' | 'en'
export type FontFamily = 'harmonyos' | 'custom'

const SETTINGS_KEY = Symbol('toolsbag-settings')
const FONT_KEY = 'toolsbag-font'
const CUSTOM_FONT_KEY = 'toolsbag-custom-font'
const WATERMARK_KEY = 'toolsbag-watermark'

const enMessages = Object.fromEntries<string>([
  ['偏好设置', 'Preferences'],
  ['打开', 'Open'],
  ['精致的桌面工具盒', 'Delicate desktop toolkit'],
  ['轻盈而克制的触感', 'Lightweight yet restrained tactility'],
  ['媒体探针', 'Media Probe'],
  ['图片工坊', 'Image Workshop'],
  ['提取视频/图片元数据，批量整理命名，并输出视频体检报告', 'Extract video/image metadata, organize names in batch, and export video health reports'],
  ['元数据提取 · 视频体检 · 批量整理', 'Metadata extraction · Video health check · Batch organize'],
  ['媒体整理', 'Media organize'],
  ['视频体检', 'Video health check'],
  ['导入视频文件，检测 MediaInfo 元数据与潜在质量异常', 'Import videos, inspect MediaInfo metadata, and detect potential quality issues'],
  ['严重', 'Critical'],
  ['警告', 'Warnings'],
  ['提示', 'Tips'],
  ['未发现异常', 'No issues found'],
  ['报告', 'report'],
  ['体检', 'Health'],
  ['读取失败', 'Read failed'],
  ['缺少详情', 'Missing detail'],
  ['无视频流', 'No video stream'],
  ['无音轨', 'No audio track'],
  ['无字幕', 'No subtitles'],
  ['时长缺失', 'Missing duration'],
  ['4K低码率', '4K low bitrate'],
  ['1080p低码率', '1080p low bitrate'],
  ['非常规帧率', 'Unusual frame rate'],
  ['HDR位深偏低', 'HDR low bit depth'],
  ['多视频流', 'Multiple video streams'],
  ['待处理', 'Ready'],
  ['不变', 'Unchanged'],
  ['非法字符', 'Invalid chars'],
  ['目标重名', 'Duplicate targets'],
  ['文件名包含非法字符，已自动替换', 'Invalid filename characters were replaced'],
  ['同批目标名称重复，将自动追加序号', 'Duplicate target names in this batch; numeric suffixes will be added'],
  ['目标文件已存在，将自动追加序号', 'Target file already exists; numeric suffix will be added'],
  ['检测到 {count} 个命名风险，已生成自动避让方案。是否继续应用？', '{count} naming risks detected. An automatic avoidance plan was generated. Continue?'],
  ['重命名预检', 'Rename precheck'],
  ['继续应用', 'Continue'],
  ['没有可撤销的重命名记录', 'No rename history to undo'],
  ['将撤销上一次重命名，共 {count} 个文件。是否继续？', 'Undo the last rename batch for {count} files. Continue?'],
  ['撤销重命名', 'Undo rename'],
  ['撤销', 'Undo'],
  ['当前文件不存在', 'Current file does not exist'],
  ['原路径已有文件，已跳过', 'Original path already has a file; skipped'],
  ['撤销失败', 'Undo failed'],
  ['撤销完成：成功 {success} 个，失败 {failed} 个', 'Undo finished: success {success}, failed {failed}'],
  ['撤销完成：成功 {success} 个', 'Undo finished: success {success}'],
  ['整理模板', 'Organize presets'],
  ['短视频素材', 'Short-video assets'],
  ['归档命名', 'Archive naming'],
  ['图片 EXIF 整理', 'Image EXIF organize'],
  ['已应用整理模板', 'Organize preset applied'],
  ['已使用本次会话缓存结果', 'Using cached result from this session'],
  ['实时导入', 'Live import'],
  ['无需刷新，已捕获最近导入', 'Instant sync · no refresh needed'],
  ['PDF合成（原样）', 'PDF Merge (As-Is)'],
  ['无损合成：按批或全部图片快速生成PDF，显示问题列表', 'Lossless merge: batch/all images to PDF with issue list'],
  ['原样嵌入 · 批量合成', 'Embed as-is · Batch merge'],
  ['文件收割', 'File Harvester'],
  ['高速复制、按格式分类与过滤，批量提取整理', 'Fast copy with grouping/filtering, batch organize'],
  ['多线程复制 · 模式过滤', 'Multithreaded copy · Pattern filters'],
  ['该工具正在路上，敬请期待', 'This tool is on the way. Stay tuned.'],
  ['主题', 'Theme'],
  ['界面风格', 'UI style'],
  ['现代风格', 'Modern style'],
  ['明暗', 'Appearance'],
  ['语言', 'Language'],
  ['浅色模式', 'Light mode'],
  ['深色模式', 'Dark mode'],
  ['Smartisan 风格', 'Smartisan style'],
  ['Apple store ui', 'Apple Store UI'],
  ['Apple 风格', 'Apple style'],
  ['Smartisan 风格默认使用木纹浅色方案', 'Smartisan style uses the wood-themed light palette'],
  ['默认使用浅色方案', 'uses the light palette'],
  ['中文', 'Chinese'],
  ['英文', 'English'],
  ['输入目录', 'Input directory'],
  ['选择需要遍历的目录', 'Select a directory to traverse'],
  ['浏览', 'Browse'],
  ['粘贴', 'Paste'],
  ['输出目录', 'Output directory'],
  ['留空则默认：同级目录 + _汇总', 'Leave empty to use sibling folder + _summary'],
  ['最小大小(MB)', 'Min size (MB)'],
  ['最大大小(MB)', 'Max size (MB)'],
  ['相同类型格式自动分组', 'Group same formats automatically'],
  ['包含模式（逗号分隔）', 'Include patterns (comma separated)'],
  ['例如：*.jpg,*.png', 'e.g. *.jpg,*.png'],
  ['排除模式（逗号分隔）', 'Exclude patterns (comma separated)'],
  ['例如：*.tmp,*.log', 'e.g. *.tmp,*.log'],
  ['开始提取', 'Start extraction'],
  ['默认输出：源目录同级的“源名_汇总”', 'Default output: sibling folder named "source_summary"'],
  ['进度', 'Progress'],
  ['总文件', 'Total files'],
  ['成功', 'Success'],
  ['失败', 'Failed'],
  ['跳过', 'Skipped'],
  ['耗时', 'Elapsed'],
  ['吞吐', 'Throughput'],
  ['格式', 'Format'],
  ['数量', 'Count'],
  ['问题日志', 'Issue log'],
  ['暂无问题', 'No issues'],
  ['剪贴板中没有有效路径', 'No valid path in clipboard'],
  ['请选择输入目录', 'Please choose input directory'],
  ['完成但存在问题文件', 'Completed with problematic files'],
  ['提取完成', 'Extraction finished'],
  ['执行失败', 'Execution failed'],
  ['无法打开目录', 'Unable to open directory'],
  ['打开所在目录', 'Open folder'],
  ['选择包含图片的文件夹', 'Choose a folder with images'],
  ['留空默认：同级生成“输入文件夹名_合并.pdf”；填写自定义名自动补 .pdf', 'Leave empty: sibling output named input_merged.pdf; custom name auto-appends .pdf'],
  ['批大小', 'Batch size'],
  ['全部合并', 'Merge all'],
  ['导入列表', 'Import list'],
  ['开始转换', 'Start conversion'],
  ['清空列表', 'Clear list'],
  ['文件名', 'File name'],
  ['大小', 'Size'],
  ['小工具说明', 'Tool note'],
  ['工具 3 说明', 'Tool 3 note'],
  ['未找到说明内容', 'No note content'],
  ['读取说明失败', 'Failed to load note'],
  ['导入失败', 'Import failed'],
  ['转换完成', 'Conversion finished'],
  ['转换失败', 'Conversion failed'],
  ['转换完成有问题', 'Conversion finished with problematic files'],
  ['无法打开输出目录', 'Unable to open output directory'],
  ['共计图片统计', 'Image count stats'],
  ['列表已清空', 'List cleared'],
  ['输入总大小', 'Input size'],
  ['输出总大小', 'Output size'],
  ['媒体列表', 'Media list'],
  ['视频', 'Video'],
  ['图片', 'Image'],
  ['信息提取', 'Info extraction'],
  ['视频与图片列表', 'Video & image list'],
  ['清空', 'Clear'],
  ['序号', 'Index'],
  ['复制名称', 'Copy name'],
  ['打开文件夹', 'Open folder'],
  ['时长', 'Duration'],
  ['分辨率', 'Resolution'],
  ['码率', 'Bitrate'],
  ['文件大小', 'File size'],
  ['预览名称', 'Preview name'],
  ['操作', 'Actions'],
  ['删除', 'Delete'],
  ['支持排序、全选、Hover 查看完整路径', 'Supports sorting, select all, hover to view full path'],
  ['支持提示', 'Supports Exif info & sorting'],
  ['采集引擎', 'Engines'],
  ['提取配置', 'Extraction settings'],
  ['视频引擎', 'Video engine'],
  ['FFprobe（默认）', 'FFprobe (default)'],
  ['时长格式', 'Duration format'],
  ['视频列显示', 'Video columns'],
  ['全选/全关', 'Toggle all'],
  ['重命名字段（视频）', 'Rename fields (video)'],
  ['上移', 'Move up'],
  ['下移', 'Move down'],
  ['图片引擎', 'Image engine'],
  ['图片列显示', 'Image columns'],
  ['拍摄设备', 'Camera'],
  ['拍摄时间', 'Taken at'],
  ['焦距', 'Focal length'],
  ['重命名字段（图片）', 'Rename fields (image)'],
  ['自定义文本', 'Custom text'],
  ['分隔符', 'Separator'],
  ['序号补零', 'Leading zeros'],
  ['重命名', 'Rename'],
  ['预览与应用', 'Preview & apply'],
  ['预览重命名', 'Preview rename'],
  ['应用重命名', 'Apply rename'],
  ['分析面板', 'Analytics'],
  ['视频概览', 'Video overview'],
  ['图片概览', 'Image overview'],
  ['基础统计', 'Basic stats'],
  ['导入视频', 'Imported videos'],
  ['总图片', 'Total images'],
  ['总时长', 'Total duration'],
  ['合计大小', 'Total size'],
  ['时长分布', 'Duration buckets'],
  ['格式分布', 'Format distribution'],
  ['极值 / 均值', 'Extremes / Averages'],
  ['最长视频', 'Longest video'],
  ['最短视频', 'Shortest video'],
  ['最高码率', 'Highest bitrate'],
  ['最低码率', 'Lowest bitrate'],
  ['平均时长', 'Avg duration'],
  ['平均码率', 'Avg bitrate'],
  ['分辨率概览', 'Resolution overview'],
  ['最大分辨率', 'Max resolution'],
  ['最小分辨率', 'Min resolution'],
  ['个', 'pcs'],
  ['批量工具 · V2.0', 'Batch tools · V2.0'],
  ['视频 / 图片批处理', 'Video / Image batch processing'],
  ['添加文件', 'Add files'],
  ['添加文件夹', 'Add folder'],
  ['粘贴路径导入', 'Paste paths to import'],
  ['处理中', 'Processing'],
  ['自动刷新', 'Auto refresh'],
  ['递归遍历', 'Recursive'],
  ['仅当前层', 'Current level only'],
  ['刷新', 'Refresh'],
  ['批次', 'Batch'],
  ['支持 Exif 信息展示与排序', 'Supports Exif info and sorting'],
  ['正在导入并提取元数据...', 'Importing and extracting metadata...'],
  ['成功 {success} 个，失败 {failed} 个；格式统计：{formats}', 'Success {success}, Failed {failed}; Formats: {formats}'],
  ['无', 'None'],
  ['导入完成', 'Import finished'],
  ['已忽略包含通配符的路径', 'Skipped paths containing wildcards'],
  ['确认从列表中移除该项吗？', 'Remove this item from the list?'],
  ['移除确认', 'Remove confirm'],
  ['移除', 'Remove'],
  ['取消', 'Cancel'],
  ['已从列表移除', 'Removed from list'],
  ['清空确认', 'Clear confirm'],
  ['已刷新统计数据', 'Stats refreshed'],
  ['已复制名称', 'Name copied'],
  ['复制失败', 'Copy failed'],
  ['打开失败', 'Open failed'],
  ['请先点击“预览重命名”', 'Please click "Preview rename" first'],
  ['没有可重命名的文件', 'No files to rename'],
  ['重命名失败', 'Rename failed'],
  ['重命名完成：成功 {success} 个，失败 {failed} 个', 'Rename finished: success {success}, failed {failed}'],
  ['重命名完成：成功 {success} 个', 'Rename finished: success {success}'],
  ['重命名异常', 'Rename error'],
  ['原始文件名', 'Original filename'],
  ['拍摄焦距', 'Focal length'],
  ['工具 3 说明', 'Tool 3 note'],
  ['界面字体', 'Font'],
  ['默认字体', 'Default'],
  ['像素字体', 'Pixel Font'],
  ['预览', 'Preview'],
  ['发现文件类型', 'Discovered file types'],
  ['全选', 'Select all'],
  ['关于', 'About'],
  // Codebook translations
  ['密码册', 'Cipher Book'],
  ['密码生成器与账号资产管理，安全存储与导出', 'Password generator & account management with secure storage'],
  ['密码生成 · 账号管理', 'Password gen · Account mgmt'],
  ['密码长度', 'Password length'],
  ['字符类型', 'Character sets'],
  ['大写字母', 'Uppercase'],
  ['小写字母', 'Lowercase'],
  ['数字', 'Numbers'],
  ['特殊符号', 'Symbols'],
  ['高级规则', 'Advanced rules'],
  ['排除易混淆字符', 'Exclude ambiguous chars'],
  ['排除代码特殊符', 'Exclude code symbols'],
  ['自定义排除', 'Custom exclude'],
  ['输入要排除的字符', 'Enter chars to exclude'],
  ['生成结果', 'Result'],
  ['点击生成密码', 'Click to generate'],
  ['生成密码', 'Generate'],
  ['复制', 'Copy'],
  ['已复制密码', 'Password copied'],
  ['名称', 'Name'],
  ['例如：WeChat, GitHub', 'e.g. WeChat, GitHub'],
  ['账号标识', 'Account identity'],
  ['选择或输入账号', 'Select or enter account'],
  ['密码', 'Password'],
  ['输入或使用生成的密码', 'Enter or use generated password'],
  ['APP/服务标签', 'APP/Service tags'],
  ['输入新标签后按回车', 'Enter new tag and press Enter'],
  ['添加', 'Add'],
  ['对应链接', 'URL'],
  ['例如：https://example.com', 'e.g. https://example.com'],
  ['备注', 'Notes'],
  ['可选备注信息', 'Optional notes'],
  ['账号入库', 'Save account'],
  ['账号已保存', 'Account saved'],
  ['已更新', 'Updated'],
  ['账号列表', 'Account list'],
  ['添加账号', 'Add account'],
  ['搜索账号...', 'Search accounts...'],
  ['导出 JSON', 'Export JSON'],
  ['导入', 'Import'],
  ['共', 'Total'],
  ['条记录', 'records'],
  ['账号', 'Account'],
  ['APP/服务', 'APP/Service'],
  ['链接', 'URL'],
  ['创建时间', 'Created'],
  ['暂无账号数据', 'No account data'],
  ['已复制', 'Copied'],
  ['确定要删除该账号吗？', 'Delete this account?'],
  ['删除确认', 'Delete confirm'],
  ['已删除', 'Deleted'],
  ['编辑账号', 'Edit account'],
  ['导出成功', 'Export success'],
  ['导入成功', 'Import success'],
  ['密码生成器', 'Password Generator'],
  ['账号信息', 'Account Info'],
  ['预设账号标识', 'Preset Identity'],
  ['邮箱', 'Email'],
  ['手机号', 'Phone'],
  ['ID/用户名', 'ID/Username'],
  ['输入新的', 'Enter new '],
  ['暂无预设', 'No presets'],
  ['暂无预设账号', 'No preset accounts'],
  ['关闭', 'Close'],
  ['附件', 'Attachments'],
  ['最多添加', 'Max'],
  ['张图片', 'images'],
  ['张', 'pcs'],
  ['请选择图片文件', 'Please select image files'],
  ['图片读取失败', 'Failed to read image'],
  ['点击或粘贴', 'Click or paste'],
  ['支持粘贴 Ctrl+V 或点击上传，最多', 'Supports Ctrl+V paste or click to upload, max'],
  ['导出', 'Export'],
  ['导出 XLSX', 'Export XLSX'],
  ['导出失败', 'Export failed'],
  ['导出预设', 'Export presets'],
  ['导入预设', 'Import presets'],
  ['条预设', 'presets'],
  ['共', 'Total'],
  ['选择格式', 'Select formats'],
  ['Image', 'Image'],
  ['Video', 'Video'],
  ['Audio', 'Audio'],
  ['Document', 'Document'],
  ['Archive', 'Archive'],
  ['Code', 'Code'],
  ['Other', 'Other'],
  ['跟随系统', 'Follow System'],
  ['鼠标点击动画', 'Click Animation'],
  ['粒子爆炸', 'Particle Explosion'],
  ['水波纹', 'Ripple'],
  ['光晕', 'Halo'],
  ['消散时间', 'Duration'],
  ['快', 'Fast'],
  ['中', 'Medium'],
  ['慢', 'Slow'],
  ['工具水印', 'Tool Watermark'],
  ['加载更多', 'Load more'],
  ['已加载全部', 'All loaded'],
  // Image-to-PDF translations
  ['选择图片文件夹', 'Select Image Folder'],
  ['分析中', 'Analyzing...'],
  ['选择文件夹', 'Select Folder'],
  ['图片分析结果', 'Image Analysis Results'],
  ['张', 'pcs'],
  ['总数量', 'Total count'],
  ['竖图', 'Portrait'],
  ['横图', 'Landscape'],
  ['总大小', 'Total size'],
  ['建议方向', 'Suggested orientation'],
  ['页面设置', 'Page Settings'],
  ['页面尺寸', 'Page size'],
  ['页面方向', 'Page orientation'],
  ['边距 (mm)', 'Margin (mm)'],
  ['压缩比例', 'Compression ratio'],
  ['不压缩，保持原始质量', 'No compression, keep original quality'],
  ['预览', 'Preview'],
  ['点击缩略图切换页面', 'Click thumbnail to switch page'],
  ['加载中', 'Loading...'],
  ['页', 'pages'],
  ['生成 PDF', 'Generate PDF'],
  ['生成中', 'Generating...'],
  ['合成 PDF', 'Compose PDF'],
  ['PDF 生成成功', 'PDF generated successfully'],
  ['页数', 'Pages'],
  ['文件大小', 'File size'],
  ['原始大小', 'Original size'],
  ['耗时', 'Elapsed'],
  ['生成失败', 'Generation failed'],
  ['分析失败', 'Analysis failed'],
  ['预览更新失败', 'Preview update failed'],
  ['页面', 'Page'],
  ['边距不能为负', 'Margin cannot be negative'],
  ['边距不能超过', 'Margin cannot exceed'],
])

const messages: Record<Locale, Record<string, string>> = {
  zh: {},
  en: enMessages
}

export const provideSettings = () => {
  const appearance = ref<Appearance>('dark')
  const skin = ref<ThemeSkin>('modern')

  // System dark mode detection
  const systemPrefersDark = ref(window.matchMedia('(prefers-color-scheme: dark)').matches)
  const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
  mediaQuery.addEventListener('change', (e) => {
    systemPrefersDark.value = e.matches
  })

  const resolvedAppearance = computed<'light' | 'dark'>(() => {
    if (appearance.value === 'system') {
      return systemPrefersDark.value ? 'dark' : 'light'
    }
    return appearance.value
  })

  const theme = computed<ThemeMode>(() => {
    return resolvedAppearance.value
  })
  const locale = ref<Locale>('zh')

  const t = (key: string, vars?: Record<string, string | number>) => {
    const base = locale.value === 'en' ? messages.en[key] ?? key : key
    if (!vars) return base
    return Object.keys(vars).reduce((msg, k) => msg.replace(new RegExp(`{${k}}`, 'g'), String(vars[k])), base)
  }

  const setTheme = (val: ThemeMode) => {
    appearance.value = val
  }

  const setSkin = (val: ThemeSkin) => {
    skin.value = val
  }

  const setAppearance = (val: Appearance) => {
    appearance.value = val
  }

  const setLocale = (val: Locale) => {
    locale.value = val
  }

  watch(
    () => theme.value,
    (val) => {
      document.documentElement.setAttribute('data-theme', val)
      if (val === 'dark') {
        document.documentElement.classList.add('dark')
      } else {
        document.documentElement.classList.remove('dark')
      }
    },
    { immediate: true }
  )

  watch(
    locale,
    (val) => {
      document.documentElement.setAttribute('lang', val)
    },
    { immediate: true }
  )

  // Built-in font switching
  const fontFamily = ref<FontFamily>((localStorage.getItem(FONT_KEY) as FontFamily) || 'harmonyos')
  const customFontName = ref<string>(localStorage.getItem(CUSTOM_FONT_KEY) || '')

  const fontFamilyMap: Record<FontFamily, string> = {
    'harmonyos': "'HarmonyOS Sans SC', -apple-system, 'SF Pro Display', 'Segoe UI', sans-serif",
    'custom': `'Custom Font', 'HarmonyOS Sans SC', -apple-system, sans-serif`
  }

  const applyFont = (font: FontFamily) => {
    let fontStack = fontFamilyMap[font]
    
    // Remove old font style if exists
    const oldStyle = document.getElementById('app-font-style')
    if (oldStyle) oldStyle.remove()

    // If custom font, inject the @font-face
    if (font === 'custom' && customFontName.value) {
      const customFontData = localStorage.getItem(`toolsbag-font-data-${customFontName.value}`)
      if (customFontData) {
        const fontFaceStyle = document.createElement('style')
        fontFaceStyle.id = 'custom-font-face'
        fontFaceStyle.textContent = `
          @font-face {
            font-family: 'Custom Font';
            src: url('${customFontData}') format('truetype');
            font-weight: 400;
            font-style: normal;
            font-display: swap;
          }
        `
        document.head.appendChild(fontFaceStyle)
      }
    } else {
      // Remove custom font face if switching away
      const customFontFace = document.getElementById('custom-font-face')
      if (customFontFace) customFontFace.remove()
    }

    document.documentElement.style.setProperty('--app-font-family', fontStack)

    // Create style to override all elements
    const style = document.createElement('style')
    style.id = 'app-font-style'
    style.textContent = `
      :root, body, .app-shell, .el-drawer, .el-dialog, .el-message, .el-menu, 
      .el-button, .el-input, .el-input__inner, .el-table, .el-tag, .el-select,
      .el-radio-button__inner, .el-checkbox, .el-form-item__label, .el-tabs,
      .el-dropdown-menu, .el-tooltip__popper, .el-notification, .el-message-box,
      .el-popover, .el-card, .el-descriptions, .el-statistic, .el-result,
      .el-empty, .el-progress, .el-switch, .el-rate, .el-slider, .el-upload,
      .el-transfer, .el-tree, .el-pagination, .el-breadcrumb, .el-steps,
      .el-alert, .el-loading-text, pre, code, textarea, input, select, button {
        font-family: ${fontStack} !important;
      }
    `
    document.head.appendChild(style)
  }

  const setFontFamily = (font: FontFamily) => {
    fontFamily.value = font
    localStorage.setItem(FONT_KEY, font)
    applyFont(font)
  }

  // Load custom font from file
  const loadCustomFont = async (file: File): Promise<boolean> => {
    return new Promise((resolve) => {
      const reader = new FileReader()
      reader.onload = (e) => {
        const dataUrl = e.target?.result as string
        if (dataUrl) {
          const fontName = file.name.replace(/\.[^.]+$/, '')
          customFontName.value = fontName
          localStorage.setItem(CUSTOM_FONT_KEY, fontName)
          localStorage.setItem(`toolsbag-font-data-${fontName}`, dataUrl)
          fontFamily.value = 'custom'
          localStorage.setItem(FONT_KEY, 'custom')
          applyFont('custom')
          resolve(true)
        } else {
          resolve(false)
        }
      }
      reader.onerror = () => resolve(false)
      reader.readAsDataURL(file)
    })
  }

  // Clear custom font
  const clearCustomFont = () => {
    if (customFontName.value) {
      localStorage.removeItem(`toolsbag-font-data-${customFontName.value}`)
    }
    customFontName.value = ''
    localStorage.removeItem(CUSTOM_FONT_KEY)
    fontFamily.value = 'harmonyos'
    localStorage.setItem(FONT_KEY, 'harmonyos')
    applyFont('harmonyos')
  }

  // Apply font on init
  applyFont(fontFamily.value)

  // Watermark setting
  const showWatermark = ref(localStorage.getItem(WATERMARK_KEY) !== 'false')
  
  const setShowWatermark = (val: boolean) => {
    showWatermark.value = val
    localStorage.setItem(WATERMARK_KEY, val ? 'true' : 'false')
  }

  const state = { 
    theme, skin, appearance, setSkin, setAppearance, setTheme, locale, setLocale, t,
    fontFamily, setFontFamily, customFontName, loadCustomFont, clearCustomFont,
    showWatermark, setShowWatermark
  }
  provide(SETTINGS_KEY, state)
  return state
}

export const useSettings = () => {
  const injected = inject<ReturnType<typeof provideSettings> | null>(SETTINGS_KEY as any, null)
  if (!injected) {
    throw new Error('Settings context not provided')
  }
  return injected
}
