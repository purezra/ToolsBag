<script setup lang="ts">
// Tilepress 样式整体收敛到 .tilepress-root，主题变量对齐 ToolsBag 设计令牌。
import "./theme.css";
import "./styles.css";
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import { Channel, invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Files } from "@element-plus/icons-vue";
import type {
  Estimate,
  ExportProgress,
  ExportResult,
  CompletedExport,
  ImageExportResult,
  ImageFormat,
  LayoutOptions,
  LayoutProgress,
  LayoutSummary,
  Mode,
  PageKind,
  PathsInfo,
  Phase,
  Prefs,
  PreviewImage,
  PreviewMode,
  PreviewProgress,
  Quality,
  Recommendation,
  ScanInfo,
  ScanProgress,
  PlanOption,
  FilterStats,
} from "./types";
import type { LayoutMode } from "./types";

const PAGE_KINDS: { key: PageKind; label: string; dims: string }[] = [
  { key: "a4p", label: "A4 竖版", dims: "210 × 297" },
  { key: "a4l", label: "A4 横版", dims: "297 × 210" },
  { key: "a3p", label: "A3 竖版", dims: "297 × 420" },
  { key: "a3l", label: "A3 横版", dims: "420 × 297" },
];

const QUALITIES: { key: Quality; label: string; note: string }[] = [
  { key: "lossless", label: "原图无损", note: "放大到原图大小也不失真 · 体积≈原图，要打印选这个" },
  { key: "high", label: "高质量", note: "肉眼看不出差别 · 体积约 1/3" },
  { key: "standard", label: "标准", note: "屏幕上翻看够用 · 体积约 1/5" },
  { key: "small", label: "小体积", note: "发微信/省空间 · 体积约 1/8" },
];

const DPI_OPTIONS: { value: number | null; label: string }[] = [
  { value: null, label: "不限" },
  { value: 150, label: "150 DPI" },
  { value: 200, label: "200 DPI" },
  { value: 300, label: "300 DPI" },
  { value: 450, label: "450 DPI" },
];

const OUTPUT_FORMATS: { key: "pdf" | ImageFormat; label: string; note: string }[] = [
  { key: "pdf", label: "PDF", note: "多页文档" },
  { key: "webp", label: "WebP", note: "支持无损" },
  { key: "avif", label: "AVIF", note: "高效压缩" },
];

// ---------- 状态 ----------
const phase = ref<Phase>("idle");
const dir = ref("");
const scan = ref<ScanInfo | null>(null);

const page = ref<PageKind>("a4p");
const margin = ref(10);
const gutter = ref(1);
const perPage = ref(53);
const perPageTouched = ref(false);
const usePages = ref(false);
const targetPages = ref(24);
const mode = ref<Mode>("fit");
const layoutMode = ref<LayoutMode>("grid");
const quality = ref<Quality>("lossless");
const dpi = ref<number | null>(null);
const previewMode = ref<PreviewMode>("scroll");

const scanProgress = ref<ScanProgress | null>(null);
const layoutProgress = ref<LayoutProgress | null>(null);
const previewProgress = ref<PreviewProgress | null>(null);
const exportProgress = ref<ExportProgress | null>(null);

const summary = ref<LayoutSummary | null>(null);
const exportResult = ref<CompletedExport | null>(null);
const copyMessage = ref("");
const copyingImages = ref(false);
const resultPath = computed(() => {
  const result = exportResult.value;
  return result ? (result.format === "pdf" ? result.pdf_path : result.out_dir) : "";
});
const resultSkipped = computed(() => {
  const result = exportResult.value;
  return result?.format === "pdf" ? result.skipped : 0;
});
const exportLabel = computed(() => exportFmt.value === "webp" ? "WebP" : exportFmt.value.toUpperCase());
const exportingPath = ref("");
const estimate = ref<Estimate | null>(null);
const rec = ref<Recommendation | null>(null);
const error = ref("");
const cancelling = ref(false);
const paths = ref<PathsInfo | null>(null);

const previews = reactive<Record<number, string>>({});
const previewLoading = reactive<Record<number, boolean>>({});
const currentPage = ref(0);
const prewarmDone = ref(false);
const prewarmCancelled = ref(false);
const prewarmCurrent = ref(0);
const prewarmTotal = ref(0);
const previewFullscreen = ref(false);

const scrollArea = ref<HTMLElement | null>(null);
const previewStage = ref<HTMLElement | null>(null);

let applyingRecommend = false;
let startedAt = 0;
const nowTick = ref(Date.now());
let clock: number | undefined;
let observer: IntersectionObserver | null = null;
let saveTimer: number | undefined;
let estimateTimer: number | undefined;
let swipeStartX: number | null = null;

// ---------- 工具 ----------
function rafThrottle<T>(apply: (v: T) => void) {
  let pending: T | undefined;
  let scheduled = false;
  return (v: T) => {
    pending = v;
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(() => {
      scheduled = false;
      if (pending !== undefined) {
        apply(pending);
        pending = undefined;
      }
    });
  };
}

function fmtSize(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${Math.round(bytes)} B`;
}

function fmtSpeed(bps: number): string {
  if (!Number.isFinite(bps) || bps < 0) return "—";
  if (bps >= 1024 ** 3) return `${(bps / 1024 ** 3).toFixed(2)} GB/s`;
  return `${(bps / 1024 ** 2).toFixed(1)} MB/s`;
}

function fmtDuration(sec: number): string {
  if (sec < 60) return `${sec.toFixed(1)} s`;
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return `${m} 分 ${s} 秒`;
}

function defaultOutput(): string {
  const sep = dir.value.includes("\\") ? "\\" : "/";
  const base = dir.value.replace(/[\\/]+$/, "");
  // 用文件夹名（最多 10 个字）做 PDF 名，例如 2026春游_a4p.pdf
  const leaf = base.split(/[\\/]/).pop() || "tilepress";
  const short =
    [...leaf]
      .slice(0, 10)
      .join("")
      .replace(/[\\/:*?"<>|]/g, "_")
      .trim() || "tilepress";
  return `${base}${sep}${short}_${page.value}.pdf`;
}

function markStart() {
  startedAt = Date.now();
  nowTick.value = Date.now();
}

const elapsedSec = computed(() =>
  startedAt ? Math.max(0, (nowTick.value - startedAt) / 1000) : 0,
);

const exportElapsed = computed(() =>
  Math.max(exportProgress.value?.elapsed_ms ? exportProgress.value.elapsed_ms / 1000 : 0, elapsedSec.value),
);

const previewElapsed = computed(() =>
  previewProgress.value ? previewProgress.value.elapsed_ms / 1000 : elapsedSec.value,
);

// ---------- 参数 ----------
const clampMargin = computed(() =>
  Math.min(50, Math.max(0, Number(margin.value) || 0)),
);
const clampGutter = computed(() =>
  Math.min(30, Math.max(0, Number(gutter.value) || 0)),
);
const safePerPage = computed(() =>
  Math.max(1, Math.round(Number(perPage.value) || 1)),
);
const safeTargetPages = computed(() =>
  Math.max(1, Math.round(Number(targetPages.value) || 1)),
);

// 相近比例优先排布（仅经典模式有效）
const clusterFirst = ref(true);

// 过滤：分辨率与长宽比（空 = 不限）
const minW = ref<number | null>(null);
const minH = ref<number | null>(null);
const arMin = ref<number | null>(null);
const arMax = ref<number | null>(null);
const filterStat = ref<FilterStats | null>(null);

// 像素阈值上限放宽到 20000（cleanLimit 是给「每页张数」用的，最多 999）
function cleanPx(v: number | null): number | null {
  const n = Number(v);
  if (!Number.isFinite(n) || n < 1) return null;
  return Math.min(20000, Math.round(n));
}
const minWv = computed(() => cleanPx(minW.value));
const minHv = computed(() => cleanPx(minH.value));
function cleanAr(v: number | null): number | null {
  const n = Number(v);
  if (!Number.isFinite(n) || n <= 0) return null;
  return Math.round(n * 100) / 100;
}
const arMinv = computed(() => cleanAr(arMin.value));
const arMaxv = computed(() => cleanAr(arMax.value));

async function refreshFilterStat() {
  if (!scan.value) {
    filterStat.value = null;
    return;
  }
  try {
    filterStat.value = await invoke<FilterStats>("filter_stats", {
      minW: minWv.value,
      minH: minHv.value,
      arMin: arMinv.value,
      arMax: arMaxv.value,
    });
  } catch {
    filterStat.value = null;
  }
}

// 推荐时每页张数的上下限（空 = 不限）
const minPerPage = ref<number | null>(null);
const maxPerPage = ref<number | null>(null);
const activeOption = ref(0);

function optLabel(o: PlanOption) {
  const nums = o.pages <= 6 ? o.plan.join(" / ") : `${o.min}–${o.max}`;
  return `${o.pages} 页 · ${nums}`;
}

function applyOption(i: number) {
  const o = rec.value?.options?.[i];
  if (!o) return;
  activeOption.value = i;
  // 这一串改动会触发 watch 清空方案，先屏蔽
  applyingRecommend = true;
  usePages.value = false;
  perPage.value = o.plan[Math.floor(o.plan.length / 2)] ?? perPage.value;
  planApplied.value = [...o.plan];
  void nextTick(() => {
    applyingRecommend = false;
  });
  perPageTouched.value = true;
  void refreshEstimate();
}

// 逐页张数：只存前 N-1 页，末页自动 = 总数 − 前面之和
const pageCounts = ref<number[]>([]);

const autoLast = computed(() => {
  const n = scan.value?.count ?? 0;
  const used = pageCounts.value.reduce((a, b) => a + (Number(b) || 0), 0);
  return n - used;
});

function resetPageCounts() {
  const n = scan.value?.count ?? 0;
  const pages = Math.min(n, safeTargetPages.value);
  const base = Math.floor(n / pages);
  const rem = n % pages;
  pageCounts.value = Array.from({ length: Math.max(0, pages - 1) }, (_, i) =>
    base + (i < rem ? 1 : 0),
  );
}

watch([safeTargetPages, () => scan.value?.count, usePages], () => {
  if (usePages.value && scan.value) resetPageCounts();
  else pageCounts.value = [];
});

const planFromInputs = computed<number[] | null>(() => {
  if (!usePages.value || !scan.value) return null;
  if (pageCounts.value.length === 0) return null;
  if (autoLast.value < 1) return null;
  return [...pageCounts.value.map((c) => Math.max(1, Math.round(Number(c) || 1))), autoLast.value];
});

const effectivePerPage = computed(() => {
  if (!scan.value) return safePerPage.value;
  if (usePages.value) {
    return Math.max(
      1,
      Math.ceil(scan.value.count / Math.min(scan.value.count, safeTargetPages.value)),
    );
  }
  return safePerPage.value;
});

// 过滤后真正会进 PDF 的张数
const keptCount = computed(() => filterStat.value?.kept ?? scan.value?.count ?? 0);

const pagesCount = computed(() => {
  if (!scan.value) return 0;
  if (usePages.value) return Math.min(keptCount.value, safeTargetPages.value);
  if (planApplied.value?.length) return planApplied.value.length;
  return Math.ceil(keptCount.value / effectivePerPage.value);
});

// 概览里显示的「每页张数」：有方案就显示方案的真实区间
const perPageLabel = computed(() => {
  const p = usePages.value ? planFromInputs.value : planApplied.value;
  if (p?.length) {
    const mn = Math.min(...p);
    const mx = Math.max(...p);
    return mn === mx ? `${mn} 张/页` : `${mn}–${mx} 张/页`;
  }
  return `${effectivePerPage.value} 张/页`;
});

const pageAspect = computed(() => {
  const kind = PAGE_KINDS.find((p) => p.key === page.value);
  const [w, h] = (kind?.dims || "210 × 297").split("×").map((s) => Number(s.trim()));
  return `${w} / ${h}`;
});

const qualityNote = computed(
  () => QUALITIES.find((q) => q.key === quality.value)?.note || "",
);

const histMax = computed(() => {
  const h = scan.value?.histogram;
  if (!h || !h.length) return 1;
  return Math.max(1, ...h.map((b) => b.count));
});
const peakRatio = computed(() => {
  const h = scan.value?.histogram;
  const first = h?.[0];
  if (!first) return "—";
  const top = h!.reduce((a, b) => (b.count > a.count ? b : a), first);
  return top.count > 0 ? `${top.ratio.toFixed(2)}（${top.count} 张）` : "—";
});
function barPct(n: number): string {
  return `${Math.max(2, Math.round((n / histMax.value) * 100))}%`;
}

const dpiNote = computed(() =>
  dpi.value === null ? "不限制输出像素" : `超过 ${dpi.value} DPI 的图会先降采样`,
);

const canLayout = computed(
  () => phase.value === "ready" && !!scan.value && scan.value.count > 0,
);

const PHASE_LABEL: Record<Phase, string> = {
  idle: "待机",
  importing: "导入中",
  ready: "参数",
  laying_out: "排版中",
  preview_ready: "预览",
  exporting: "导出中",
  done: "完成",
};

const phaseLabel = computed(() => PHASE_LABEL[phase.value]);

const planApplied = ref<number[] | null>(null);

function buildOpts(): LayoutOptions {
  return {
    page: page.value,
    layout: layoutMode.value,
    margin_mm: clampMargin.value,
    gutter_mm: clampGutter.value,
    per_page: safePerPage.value,
    per_page_plan: planFromInputs.value ?? planApplied.value,
    pages: usePages.value ? safeTargetPages.value : null,
    mode: mode.value,
    quality: quality.value,
    dpi: dpi.value,
    seed: 42,
    iters: 40000,
    even: true,
    cluster: clusterFirst.value,
    min_w: minWv.value,
    min_h: minHv.value,
    ar_min: arMinv.value,
    ar_max: arMaxv.value,
  };
}

async function refreshRecommend() {
  if (!scan.value) {
    rec.value = null;
    return;
  }
  try {
    rec.value = await invoke<Recommendation>("recommend", {
      page: page.value,
      marginMm: clampMargin.value,
      gutterMm: gutter.value,
      minPer: minLimit.value,
      maxPer: maxLimit.value,
      minW: minWv.value,
      minH: minHv.value,
      arMin: arMinv.value,
      arMax: arMaxv.value,
    });
    if (!perPageTouched.value && rec.value.plan?.length) {
      applyingRecommend = true;
      perPage.value = rec.value.mid;
      planApplied.value = [...rec.value.plan];
      void nextTick(() => {
        applyingRecommend = false;
      });
      void refreshEstimate();
    }
  } catch {
    rec.value = null;
  }
}

async function refreshEstimate() {
  if (!scan.value) {
    estimate.value = null;
    return;
  }
  try {
    estimate.value = await invoke<Estimate>("estimate_export", { opts: buildOpts() });
  } catch {
    estimate.value = null;
  }
}

function applyRecommend() {
  if (!rec.value || !rec.value.plan?.length) return;
  usePages.value = false;
  planApplied.value = [...rec.value.plan];
  perPage.value = rec.value.mid;
  perPageTouched.value = true;
  void refreshEstimate();
}

function clearPlan() {
  if (planApplied.value) planApplied.value = null;
}

// 手输上下限：防抖 300ms，避免每敲一个字符就打一次推荐（也避免半截数字触发异常）
let limitTimer: number | undefined;
function cleanLimit(v: number | null): number | null {
  const n = Number(v);
  if (!Number.isFinite(n) || n < 1) return null;
  return Math.min(999, Math.round(n));
}
const minLimit = computed(() => cleanLimit(minPerPage.value));
const maxLimit = computed(() => cleanLimit(maxPerPage.value));

let filterTimer: number | undefined;
watch([minWv, minHv, arMinv, arMaxv], () => {
  if (filterTimer) clearTimeout(filterTimer);
  filterTimer = window.setTimeout(() => {
    activeOption.value = 0;
    perPageTouched.value = false;
    void refreshFilterStat();
    void refreshRecommend();
    void refreshEstimate();
  }, 350);
});

watch([minLimit, maxLimit], () => {
  if (limitTimer) clearTimeout(limitTimer);
  limitTimer = window.setTimeout(() => {
    activeOption.value = 0;
    void refreshRecommend();
  }, 300);
});

watch([perPage, usePages, targetPages, margin, gutter, page, layoutMode], () => {
  if (applyingRecommend) return;
  // 方案自己设置的中位数不算用户改动
  if (planApplied.value?.length) {
    const mid = planApplied.value[Math.floor(planApplied.value.length / 2)];
    if (perPage.value === mid && !usePages.value) return;
  }
  clearPlan();
});

// ---------- 配置 ----------
function applyPrefs(p: Prefs) {
  if (PAGE_KINDS.some((x) => x.key === p.page)) page.value = p.page;
  if (Number.isFinite(p.margin_mm)) margin.value = p.margin_mm;
  if (Number.isFinite(p.gutter_mm)) gutter.value = p.gutter_mm;
  if (p.mode === "cover" || p.mode === "fit") mode.value = p.mode;
  if (p.layout === "grid" || p.layout === "mosaic") layoutMode.value = p.layout;
  if (p.quality) quality.value = p.quality;
  dpi.value = p.dpi ?? null;
  if (p.per_page && p.per_page >= 1) {
    // 只是恢复上次的默认值，不算手动指定，推荐方案仍会自动生效
    applyingRecommend = true;
    perPage.value = p.per_page;
    void nextTick(() => {
      applyingRecommend = false;
    });
  }
  previewMode.value = p.preview_mode === "swipe" ? "swipe" : "scroll";
  minW.value = p.min_w ?? null;
  minH.value = p.min_h ?? null;
  arMin.value = p.ar_min ?? null;
  arMax.value = p.ar_max ?? null;
  if (OUTPUT_FORMATS.some((f) => f.key === p.export_format)) exportFmt.value = p.export_format!;
  imgDpi.value = p.image_dpi ?? null;
  imgQuality.value = p.image_quality === 92 ? 92 : 100;
}

async function savePrefs() {
  try {
    await invoke("save_config", {
      prefs: {
        page: page.value,
        margin_mm: clampMargin.value,
        gutter_mm: clampGutter.value,
        mode: mode.value,
        layout: layoutMode.value,
        per_page: safePerPage.value,
        quality: quality.value,
        dpi: dpi.value,
        preview_mode: previewMode.value,
        min_w: minWv.value,
        min_h: minHv.value,
        ar_min: arMinv.value,
        ar_max: arMaxv.value,
        export_format: exportFmt.value,
        image_dpi: imgDpi.value,
        image_quality: imgQuality.value,
      } satisfies Prefs,
    });
  } catch {
    // 配置保存失败不影响主流程
  }
}

// ---------- 导入 ----------
function resetResults() {
  scan.value = null;
  summary.value = null;
  exportResult.value = null;
  copyMessage.value = "";
  estimate.value = null;
  rec.value = null;
  scanProgress.value = null;
  layoutProgress.value = null;
  previewProgress.value = null;
  exportProgress.value = null;
  prewarmDone.value = false;
  prewarmCancelled.value = false;
  prewarmCurrent.value = 0;
  prewarmTotal.value = 0;
  currentPage.value = 0;
  Object.keys(previews).forEach((k) => delete previews[Number(k)]);
  Object.keys(previewLoading).forEach((k) => delete previewLoading[Number(k)]);
  error.value = "";
}

async function chooseFolder() {
  if (phase.value === "importing") return;
  try {
    const picked = await open({
      directory: true,
      multiple: false,
      title: "选择图片文件夹",
    });
    if (typeof picked === "string") await importFolder(picked);
  } catch (e) {
    error.value = `打开文件夹选择器失败：${String(e)}`;
  }
}

async function importFolder(path: string) {
  resetResults();
  dir.value = path;
  phase.value = "importing";
  cancelling.value = false;
  markStart();

  const push = rafThrottle<ScanProgress>((v) => (scanProgress.value = v));
  const channel = new Channel<ScanProgress>();
  channel.onmessage = push;

  try {
    const info = await invoke<ScanInfo>("scan_folder", { dir: path, onProgress: channel });
    scan.value = info;
    phase.value = "ready";
    if (info.count === 0) {
      error.value =
        "该文件夹（含子文件夹）里没有可识别的图片。支持 jpg / png / webp / bmp / gif / tiff。";
    } else {
      await refreshFilterStat();
      await refreshRecommend();
      await refreshEstimate();
    }
  } catch (e) {
    if (String(e).includes("已取消")) {
      phase.value = "idle";
    } else {
      error.value = String(e);
      phase.value = "idle";
    }
  } finally {
    cancelling.value = false;
  }
}

async function cancelCurrent() {
  if (cancelling.value) return;
  cancelling.value = true;
  try {
    await invoke("cancel_current");
  } catch {
    // 忽略
  }
}

// ---------- 排版 ----------
async function startLayout() {
  if (!canLayout.value) return;
  error.value = "";
  cancelling.value = false;
  phase.value = "laying_out";
  markStart();

  const push = rafThrottle<LayoutProgress>((v) => (layoutProgress.value = v));
  const channel = new Channel<LayoutProgress>();
  channel.onmessage = push;

  try {
    const s = await invoke<LayoutSummary>("build_layout_cmd", {
      opts: buildOpts(),
      onProgress: channel,
    });
    summary.value = s;
    Object.keys(previews).forEach((k) => delete previews[Number(k)]);
    prewarmDone.value = false;
    prewarmCancelled.value = false;
    prewarmCurrent.value = 0;
    prewarmTotal.value = s.pages;
    previewProgress.value = null;
    currentPage.value = 0;
    phase.value = "preview_ready";
    await nextTick();
    setupScrollObserver();
    void startPrewarm();
  } catch (e) {
    if (String(e).includes("已取消")) {
      phase.value = "ready";
    } else {
      error.value = String(e);
      phase.value = "ready";
    }
  } finally {
    cancelling.value = false;
  }
}

async function startPrewarm() {
  if (!summary.value) return;
  const push = rafThrottle<PreviewProgress>((v) => {
    previewProgress.value = v;
    prewarmCurrent.value = v.current;
    prewarmTotal.value = v.total;
  });
  const channel = new Channel<PreviewProgress>();
  channel.onmessage = push;
  try {
    const done = await invoke<number>("prewarm_previews", { onProgress: channel });
    if (done >= prewarmTotal.value) {
      prewarmDone.value = true;
    } else {
      prewarmCancelled.value = true;
    }
  } catch (e) {
    if (String(e).includes("已取消")) {
      prewarmCancelled.value = true;
    } else {
      // 预生成失败不致命，按需渲染仍可用
      prewarmCancelled.value = true;
    }
  } finally {
    loadVisiblePreviews();
  }
}

// ---------- 预览 ----------
async function ensurePreview(idx: number) {
  if (!summary.value || idx < 0 || idx >= summary.value.pages) return;
  if (previews[idx] || previewLoading[idx]) return;
  previewLoading[idx] = true;
  try {
    const img = await invoke<PreviewImage>("render_page_preview", { pageIdx: idx });
    previews[idx] = img.data_url;
  } catch {
    // 单页失败只影响该页占位
  } finally {
    previewLoading[idx] = false;
  }
}

function loadVisiblePreviews() {
  if (!summary.value) return;
  if (previewMode.value === "swipe") {
    ensurePreview(currentPage.value);
    ensurePreview(currentPage.value + 1);
    ensurePreview(currentPage.value - 1);
  } else {
    for (let i = 0; i < Math.min(3, summary.value.pages); i++) ensurePreview(i);
  }
}

function setupScrollObserver() {
  if (previewMode.value !== "scroll") return;
  nextTick(() => {
    observer?.disconnect();
    if (!scrollArea.value) return;
    observer = new IntersectionObserver(
      (entries) => {
        for (const en of entries) {
          if (!en.isIntersecting) continue;
          const idx = Number((en.target as HTMLElement).dataset.page);
          if (Number.isFinite(idx)) {
            currentPage.value = idx;
            ensurePreview(idx);
            ensurePreview(idx + 1);
          }
        }
      },
      { root: scrollArea.value, threshold: 0.45 },
    );
    scrollArea.value
      .querySelectorAll<HTMLElement>("[data-page]")
      .forEach((el) => observer?.observe(el));
  });
}

function switchPreviewMode(m: PreviewMode) {
  previewMode.value = m;
  nextTick(() => {
    if (m === "scroll") {
      setupScrollObserver();
    } else {
      ensurePreview(currentPage.value);
    }
  });
}

function goPage(delta: number) {
  if (!summary.value) return;
  const next = Math.min(summary.value.pages - 1, Math.max(0, currentPage.value + delta));
  currentPage.value = next;
  ensurePreview(next);
}

function onSwipeStart(e: PointerEvent) {
  swipeStartX = e.clientX;
}

function onSwipeEnd(e: PointerEvent) {
  if (swipeStartX === null) return;
  const dx = e.clientX - swipeStartX;
  swipeStartX = null;
  if (Math.abs(dx) < 50) return;
  goPage(dx < 0 ? 1 : -1);
}

async function toggleFullscreen() {
  try {
    if (document.fullscreenElement) {
      await document.exitFullscreen();
    } else if (previewStage.value) {
      await previewStage.value.requestFullscreen();
    }
  } catch {
    // 忽略
  }
}

function onFullscreenChange() {
  previewFullscreen.value = !!document.fullscreenElement;
}

function onKeydown(e: KeyboardEvent) {
  if (phase.value !== "preview_ready" || previewMode.value !== "swipe") return;
  if (e.key === "ArrowRight") goPage(1);
  if (e.key === "ArrowLeft") goPage(-1);
}

function backToParams() {
  if (phase.value === "preview_ready" && !prewarmDone.value) {
    void invoke("cancel_current");
  }
  phase.value = "ready";
  observer?.disconnect();
}

function backToPreview() {
  if (!summary.value) return;
  phase.value = "preview_ready";
  nextTick(() => setupScrollObserver());
}

// ---------- 导出 ----------
// 当前文件夹名（导出图片时做前缀）
const dirName = computed(() => {
  const base = (dir.value || "").replace(/[\\/]+$/, "");
  return base.split(/[\\/]/).pop() || "tilepress";
});

// 导出格式：pdf / webp / avif
const exportFmt = ref<"pdf" | ImageFormat>("pdf");
const imgDpi = ref<number | null>(null);
const imgQuality = ref(100);
const imageQualityLabel = computed(() => imgQuality.value === 100
  ? (exportFmt.value === "webp" ? "无损编码" : "最高质量") : "高质量压缩");
const imageResolutionLabel = computed(() => imgDpi.value === null ? "保留原图像素" : `${imgDpi.value} DPI`);
const imageQualityNote = computed(() => {
  if (imgQuality.value !== 100) return "有损压缩，兼顾清晰度和文件体积。";
  return exportFmt.value === "webp"
    ? "拼图像素无损保存，避免二次压缩损伤细字；文件通常更大。"
    : "质量 100、完整色彩采样和 RGB 编码；均衡提速，文件可能略大，仍为有损格式。";
});

async function startExportImages() {
  if (!summary.value) return;
  const format = exportFmt.value;
  if (format === "pdf") return;
  let dir: string | null = null;
  try {
    dir = await open({ directory: true, title: "选择图片输出文件夹" });
  } catch (e) {
    error.value = `选择输出文件夹失败：${String(e)}`;
    return;
  }
  if (typeof dir !== "string") return;

  if (!prewarmDone.value) {
    try {
      await invoke("cancel_current");
    } catch {
      // 忽略
    }
  }

  const base = (() => {
    const leaf = dirName.value || "tilepress";
    return [...leaf].slice(0, 10).join("").replace(/[\\/:*?"<>|]/g, "_").trim() || "tilepress";
  })();

  error.value = "";
  cancelling.value = false;
  exportProgress.value = null;
  copyMessage.value = "";
  exportingPath.value = dir;
  phase.value = "exporting";
  markStart();

  const channel = new Channel<ExportProgress>();
  channel.onmessage = rafThrottle<ExportProgress>((v) => (exportProgress.value = v));

  try {
    const r = await invoke<ImageExportResult>(
      "export_images_cmd",
      {
        req: {
          fmt: format,
          dpi: imgDpi.value,
          quality: imgQuality.value,
          out_dir: dir,
          base_name: base,
        },
        onProgress: channel,
      },
    );
    exportResult.value = { ...r, format };
    phase.value = "done";
  } catch (e) {
    if (String(e).includes("已取消")) {
      phase.value = "preview_ready";
    } else {
      error.value = String(e);
      phase.value = "preview_ready";
    }
  } finally {
    cancelling.value = false;
  }
}

async function startExport() {
  if (exportFmt.value !== "pdf") {
    await startExportImages();
    return;
  }
  if (!summary.value) return;
  let out: string | null = null;
  try {
    out = await save({
      defaultPath: defaultOutput(),
      title: "保存 PDF",
      filters: [{ name: "PDF 文件", extensions: ["pdf"] }],
    });
  } catch (e) {
    error.value = `选择输出路径失败：${String(e)}`;
    return;
  }
  if (typeof out !== "string") return;

  // 预生成还没结束就先停掉，避免和导出抢磁盘
  if (!prewarmDone.value) {
    try {
      await invoke("cancel_current");
    } catch {
      // 忽略
    }
  }

  exportingPath.value = out;
  error.value = "";
  cancelling.value = false;
  exportProgress.value = null;
  copyMessage.value = "";
  phase.value = "exporting";
  markStart();

  const push = rafThrottle<ExportProgress>((v) => (exportProgress.value = v));
  const channel = new Channel<ExportProgress>();
  channel.onmessage = push;

  try {
    const r = await invoke<ExportResult>("export_pdf_cmd", {
      outPath: out,
      onProgress: channel,
    });
    exportResult.value = { ...r, format: "pdf" };
    phase.value = "done";
  } catch (e) {
    if (String(e).includes("已取消")) {
      phase.value = "preview_ready";
    } else {
      error.value = String(e);
      phase.value = "preview_ready";
    }
  } finally {
    cancelling.value = false;
  }
}

async function openPdf() {
  if (exportResult.value?.format !== "pdf") return;
  try {
    await invoke("open_pdf", { path: exportResult.value.pdf_path });
  } catch (e) {
    error.value = `打开 PDF 失败：${String(e)}`;
  }
}

async function revealPdf() {
  if (exportResult.value?.format !== "pdf") return;
  try {
    await invoke("reveal_pdf", { path: exportResult.value.pdf_path });
  } catch (e) {
    error.value = `打开文件夹失败：${String(e)}`;
  }
}

async function openImageFolder() {
  const result = exportResult.value;
  if (!result || result.format === "pdf") return;
  try {
    await invoke("open_output_folder", { path: result.out_dir });
  } catch (e) {
    error.value = `打开图片文件夹失败：${String(e)}`;
  }
}

async function copyAllImages() {
  const result = exportResult.value;
  if (!result || result.format === "pdf" || copyingImages.value) return;
  copyingImages.value = true;
  copyMessage.value = "";
  error.value = "";
  try {
    await invoke("copy_image_files", { paths: result.files });
    copyMessage.value = `已复制 ${result.files.length} 张图片文件，可粘贴到文件夹`;
  } catch (e) {
    error.value = `复制图片失败：${String(e)}`;
  } finally {
    copyingImages.value = false;
  }
}

// ---------- 生命周期 ----------
onMounted(async () => {
  clock = window.setInterval(() => {
    nowTick.value = Date.now();
  }, 200);
  document.addEventListener("fullscreenchange", onFullscreenChange);
  window.addEventListener("keydown", onKeydown);

  try {
    paths.value = await invoke<PathsInfo>("app_paths");
  } catch {
    paths.value = null;
  }
  try {
    const prefs = await invoke<Prefs>("load_config");
    applyPrefs(prefs);
  } catch {
    // 浏览器预览等环境忽略
  }
  try {
    const startup = await invoke<string | null>("startup_dir");
    if (startup) await importFolder(startup);
  } catch {
    // 忽略
  }
});

onUnmounted(() => {
  if (clock) window.clearInterval(clock);
  observer?.disconnect();
  document.removeEventListener("fullscreenchange", onFullscreenChange);
  window.removeEventListener("keydown", onKeydown);
});

watch([page, margin], () => {
  void refreshRecommend();
});

watch(
  [page, margin, gutter, perPage, usePages, targetPages, mode, layoutMode, quality, dpi],
  () => {
    if (scan.value) {
      window.clearTimeout(estimateTimer);
      estimateTimer = window.setTimeout(() => void refreshEstimate(), 250);
    }
  },
);

watch(
  [page, margin, gutter, mode, layoutMode, perPage, quality, dpi, previewMode, exportFmt, imgDpi, imgQuality],
  () => {
    window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => void savePrefs(), 500);
  },
);

watch(
  perPage,
  () => {
    if (!applyingRecommend) perPageTouched.value = true;
  },
  { flush: "sync" },
);
</script>

<template>
  <div class="tilepress-root">
  <div class="app">
    <header v-if="phase !== 'idle'" class="topbar">
      <div class="top-meta">
        <span v-if="paths?.portable" class="pill ok" :title="paths?.config_path">便携模式</span>
        <span v-if="scan" class="pill">{{ scan.count }} 张</span>
        <span v-if="summary" class="pill">{{ summary.pages }} 页</span>
        <span class="pill stage-pill">{{ phaseLabel }}</span>
      </div>
    </header>

    <!-- ① 导入中 -->
    <main v-if="phase === 'importing'" class="center-stage">
      <section class="task-card">
        <h2>正在导入图片</h2>
        <p class="task-sub">{{ dir }}</p>
        <div class="progress-track big">
          <div class="progress-bar" :class="{ complete: (scanProgress?.percent || 0) >= 100 }" :style="{ width: (scanProgress?.percent || 0) + '%' }"></div>
        </div>
        <div class="progress-line">
          <span>{{ scanProgress?.message || "正在扫描…" }}</span>
          <span class="mono">{{ Math.round(scanProgress?.percent || 0) }}%</span>
        </div>
        <div class="stat-row">
          <div><span>已扫描</span><b>{{ scanProgress?.scanned || 0 }} / {{ scanProgress?.total || 0 }}</b></div>
          <div><span>可用图片</span><b>{{ scanProgress?.usable || 0 }}</b></div>
          <div><span>已读大小</span><b>{{ fmtSize(scanProgress?.bytes || 0) }}</b></div>
          <div><span>已用时间</span><b>{{ fmtDuration(elapsedSec) }}</b></div>
        </div>
        <button class="btn danger" :disabled="cancelling" @click="cancelCurrent">
          {{ cancelling ? "正在取消…" : "取消导入" }}
        </button>
      </section>
    </main>

    <!-- 空闲 -->
    <main v-else-if="phase === 'idle'" class="center-stage">
      <section class="idle-card">
        <div class="idle-mark" aria-hidden="true">
          <el-icon :size="40"><Files /></el-icon>
        </div>
        <h2>选择一个装满图片的文件夹</h2>
        <p class="muted">
          支持 JPG / PNG / WebP / BMP / GIF / TIFF，可含子文件夹。<br />
          导入只读取尺寸，不会显示缩略图，也不会修改原图。
        </p>
        <button class="btn primary big" @click="chooseFolder">选择图片文件夹…</button>
        <p v-if="error" class="error" role="alert">{{ error }}</p>
        <p v-if="paths" class="path-note mono" :title="paths.data_dir">
          配置：{{ paths.config_path }}
        </p>
      </section>
    </main>

    <!-- ② 参数 -->
    <main v-else-if="phase === 'ready'" class="workspace">
      <div class="workspace-body">
        <section class="panel params">
          <div class="panel-head">
            <h2>排版参数</h2>
            <span class="muted">导入完成 · {{ fmtDuration(scan ? scan.ms_scan / 1000 : 0) }}</span>
          </div>
          <div class="panel-body">
            <div class="field">
              <label>图片文件夹</label>
              <div class="row">
                <div class="path" :title="dir">{{ dir }}</div>
                <button class="btn" @click="chooseFolder">更换文件夹</button>
              </div>
              <p v-if="scan" class="hint">
                共 {{ scan.count }} 张图片 · 合计 {{ fmtSize(scan.total_bytes) }} ·
                平均 {{ fmtSize(scan.avg_bytes) }}/张
                <span v-if="scan.skipped"> · 跳过 {{ scan.skipped }} 个无法识别的文件</span>
              </p>
            </div>

            <div class="section-heading"><h3>纸张与排版</h3><span>先选版式，再调留白</span></div>
            <fieldset class="field">
              <legend>纸张幅面</legend>
              <div class="segmented four">
                <label v-for="p in PAGE_KINDS" :key="p.key" :class="{ active: page === p.key }">
                  <input v-model="page" type="radio" name="page" :value="p.key" />
                  <span class="seg-title">{{ p.label }}</span>
                  <span class="seg-sub">{{ p.dims }} mm</span>
                </label>
              </div>
            </fieldset>

            <fieldset class="field">
              <legend>排版方式（怎么排）</legend>
              <div class="segmented two">
                <label :class="{ active: layoutMode === 'grid' }">
                  <input v-model="layoutMode" type="radio" name="layout" value="grid" />
                  <span class="seg-title">经典网格（推荐）</span>
                  <span class="seg-sub">不裁剪 · 保留完整内容</span>
                </label>
                <label :class="{ active: layoutMode === 'mosaic' }">
                  <input v-model="layoutMode" type="radio" name="layout" value="mosaic" />
                  <span class="seg-title">自由拼贴</span>
                  <span class="seg-sub">填得最满 · 可能裁掉一点边</span>
                </label>
              </div>
            </fieldset>
            <fieldset v-if="layoutMode === 'mosaic'" class="field">
              <legend>拼贴模式（要不要裁边）</legend>
              <div class="segmented two">
                <label :class="{ active: mode === 'cover' }">
                  <input v-model="mode" type="radio" name="mode" value="cover" />
                  <span class="seg-title">铺满</span>
                  <span class="seg-sub">裁掉溢出部分 · 纸面无白缝</span>
                </label>
                <label :class="{ active: mode === 'fit' }">
                  <input v-model="mode" type="radio" name="mode" value="fit" />
                  <span class="seg-title">贴合</span>
                  <span class="seg-sub">不裁剪 · 格子边留白</span>
                </label>
              </div>
            </fieldset>

            <div class="field-grid three">
              <div class="field">
                <label for="margin">页边距</label>
                <div class="num">
                  <input id="margin" v-model.number="margin" type="number" min="0" max="50" step="0.5" />
                  <span>mm</span>
                </div>
              </div>
              <div class="field">
                <label for="gutter">图片间距</label>
                <div class="num">
                  <input id="gutter" v-model.number="gutter" type="number" min="0" max="30" step="0.5" />
                  <span>mm</span>
                </div>
              </div>
              <div class="field">
                <div class="label-row">
                  <label for="perpage">每页张数</label>
                  <button v-if="rec && rec.plan?.length" class="link" @click="applyRecommend" title="按推荐的最优逐页张数排版">用推荐方案</button>
                </div>
                <div class="num">
                  <input id="perpage" v-model.number="perPage" type="number" min="1" step="1" :disabled="usePages" />
                  <span>张</span>
                </div>
              </div>
            </div>
            <div class="section-heading"><h3>分页安排</h3><span>推荐方案可直接选用</span></div>
            <div class="field">
              <label class="switch">
                <input v-model="usePages" type="checkbox" />
                <span>先定总页数（逐页张数可调）</span>
              </label>
              <div v-if="usePages" class="num">
                <input v-model.number="targetPages" type="number" min="1" step="1" />
                <span>页</span>
              </div>
              <div v-if="usePages && scan && safeTargetPages > 1" class="plan-edit">
                <div class="plan-row">
                  <span class="plan-tag">每页张数</span>
                  <input
                    v-for="(_c, i) in pageCounts"
                    :key="i"
                    v-model.number="pageCounts[i]"
                    type="number"
                    min="1"
                    step="1"
                    class="plan-in"
                    :title="`第 ${i + 1} 页`"
                  />
                  <span class="plan-auto" :class="{ bad: autoLast < 1 }" :title="`第 ${safeTargetPages} 页（自动）`">
                    {{ autoLast }}
                  </span>
                  <button class="link" @click="resetPageCounts" title="把所有页恢复成平均分配">恢复均分</button>
                </div>
                <p class="hint tight">
                  前 {{ safeTargetPages - 1 }} 页可改，末页自动 =
                  {{ scan.count }} − 前面之和 = <b>{{ autoLast }}</b>
                  <template v-if="autoLast < 1">（超出总数，请调小前面的张数）</template>
                </p>
              </div>
            </div>

            <div class="rec-box" v-if="rec && rec.options && rec.options.length">
              <div class="rec-head">
                <span>推荐方案</span>
                <span class="rec-limits">
                  每页
                  <input v-model.number="minPerPage" type="number" min="1" step="1" placeholder="最少" class="lim-in" />
                  –
                  <input v-model.number="maxPerPage" type="number" min="1" step="1" placeholder="最多" class="lim-in" />
                  张
                </span>
              </div>
              <label
                v-for="(o, i) in rec.options"
                :key="i"
                class="rec-opt"
                :class="{ active: activeOption === i }"
              >
                <input type="radio" :checked="activeOption === i" @change="applyOption(i)" />
                <span class="rec-name">方案 {{ i + 1 }}</span>
                <span class="rec-nums">{{ optLabel(o) }}</span>
                <span class="rec-meta">填充 {{ (o.fill * 100).toFixed(1) }}% · 面积差 {{ o.spread.toFixed(2) }} 倍</span>
              </label>
              <p v-if="rec.options.some((o) => o.relaxed)" class="hint tight warn">
                这个上下限凑不出来（例如 60 张要求每页 40–45 张），下面是最接近的方案
              </p>
              <p v-else class="hint tight">点一行即按该方案排版；不填上下限就自动算最饱满的方案</p>
            </div>

            <details class="advanced-settings">
              <summary>排序与图片过滤 <span>可选</span></summary>
              <div class="advanced-body">
            <div class="field">
              <label class="switch">
                <input v-model="clusterFirst" type="checkbox" :disabled="layoutMode !== 'grid'" />
                <span>相近比例优先排布</span>
              </label>
              <p class="hint tight">
                勾选 → 相近比例凑一页：前几页特别整齐，零散比例集中到最后几页<br />
                不勾选 → 按导入顺序摊到各页：每页参差程度差不多
              </p>
            </div>

            <div class="filter-box">
              <div class="filter-head">
                <span>过滤图片</span>
                <span v-if="filterStat" class="filter-stat" :class="{ dim: filterStat.kept === filterStat.total }">
                  保留 {{ filterStat.kept }} / {{ filterStat.total }} 张
                  <template v-if="filterStat.total > filterStat.kept">（滤掉 {{ filterStat.total - filterStat.kept }} 张）</template>
                </span>
              </div>
              <div class="filter-row">
                <span class="filter-label">分辨率至少</span>
                <input v-model.number="minW" type="number" min="1" step="1" placeholder="宽" class="lim-in wide" />
                <span class="times">×</span>
                <input v-model.number="minH" type="number" min="1" step="1" placeholder="高" class="lim-in wide" />
                <span class="unit">px</span>
              </div>
              <div class="filter-row">
                <span class="filter-label">宽高比</span>
                <input v-model.number="arMin" type="number" min="0.01" step="0.01" placeholder="最小" class="lim-in wide" />
                <span class="times">–</span>
                <input v-model.number="arMax" type="number" min="0.01" step="0.01" placeholder="最大" class="lim-in wide" />
                <span class="unit">宽/高</span>
              </div>
              <p class="hint tight">
                留空 = 不过滤。宽高比 &lt;1 是竖图，1 是方图，&gt;1 是横图；例：只想留横图填 1 – 99
              </p>
            </div>
              </div>
            </details>
            <p v-if="error" class="error" role="alert">{{ error }}</p>
          </div>
        </section>

        <aside class="panel overview">
          <div class="panel-head"><h2>导出设置</h2><span class="muted">预览后导出</span></div>
          <div class="panel-body">
            <fieldset class="field output-formats">
              <legend>输出格式</legend>
              <div class="segmented three">
                <label v-for="f in OUTPUT_FORMATS" :key="f.key" :class="{ active: exportFmt === f.key }">
                  <input v-model="exportFmt" type="radio" name="export-format" :value="f.key" />
                  <span class="seg-title">{{ f.label }}</span><span class="seg-sub">{{ f.note }}</span>
                </label>
              </div>
            </fieldset>
            <template v-if="exportFmt !== 'pdf'">
              <fieldset class="field image-quality">
                <legend>图片清晰度</legend>
                <div class="segmented two">
                  <label :class="{ active: imgQuality === 100 }">
                    <input v-model="imgQuality" type="radio" name="image-quality" :value="100" />
                    <span class="seg-title">最高保真</span>
                    <span class="seg-sub">{{ exportFmt === 'webp' ? '无损编码 · 推荐' : '最高质量 · 4:4:4' }}</span>
                  </label>
                  <label :class="{ active: imgQuality === 92 }">
                    <input v-model="imgQuality" type="radio" name="image-quality" :value="92" />
                    <span class="seg-title">高质量压缩</span><span class="seg-sub">更小体积 · 有损</span>
                  </label>
                </div>
              </fieldset>
              <div class="field">
                <label for="image-dpi">输出分辨率</label>
                <select id="image-dpi" v-model="imgDpi" class="select">
                  <option :value="null">保留原图像素（推荐）</option>
                  <option v-for="n in [150, 200, 300, 450, 600]" :key="n" :value="n">{{ n }} DPI</option>
                </select>
                <p class="hint">{{ imgDpi === null ? '按原图尺寸自动确定每页分辨率，避免缩小细字。' : '指定整页分辨率，可能会缩小原图。' }}</p>
              </div>
              <div class="quality-explanation"><b>{{ imageQualityLabel }}</b><p>{{ imageQualityNote }}</p></div>
            </template>
            <template v-else>
            <fieldset class="field">
              <legend>输出质量（画质与体积）</legend>
              <div class="quality-grid">
                <label v-for="q in QUALITIES" :key="q.key" :class="{ active: quality === q.key }">
                  <input v-model="quality" type="radio" name="quality" :value="q.key" />
                  <span class="seg-title">{{ q.label }}</span>
                  <span class="seg-sub">{{ q.note }}</span>
                </label>
              </div>
            </fieldset>

            <div class="field">
              <label for="dpi">输出 DPI 上限</label>
              <select id="dpi" v-model="dpi" class="select">
                <option v-for="d in DPI_OPTIONS" :key="String(d.value)" :value="d.value">
                  {{ d.label }}
                </option>
              </select>
              <p class="hint">{{ dpiNote }}</p>
            </div>

            </template>
            <div class="section-heading"><h3>任务概览</h3><span>{{ keptCount }} 张图片</span></div>

            <div class="big-stat">
              <b>{{ pagesCount }}</b>
              <span>页</span>
              <em>{{ perPageLabel }}</em>
            </div>
            <details class="image-statistics"><summary>图片统计与比例分布</summary>
            <div class="stat-grid">
              <div><span>图片</span><b>{{ scan?.count || 0 }}</b></div>
              <div><span>合计</span><b>{{ fmtSize(scan?.total_bytes || 0) }}</b></div>
              <div><span>横版</span><b>{{ scan?.landscape || 0 }}</b></div>
              <div><span>竖版</span><b>{{ scan?.portrait || 0 }}</b></div>
              <div><span>方图</span><b>{{ scan?.square || 0 }}</b></div>
              <div><span>平均</span><b>{{ fmtSize(scan?.avg_bytes || 0) }}</b></div>
            </div>

            <div v-if="scan?.histogram?.length" class="hist">
              <div class="hist-title">
                长宽比分布
                <span class="hist-peak">最集中 {{ peakRatio }}</span>
              </div>
              <div class="hist-bars">
                <div
                  v-for="(b, i) in scan.histogram"
                  :key="i"
                  class="hist-bar"
                  :class="{ peak: b.count === histMax && b.count > 0 }"
                  :style="{ height: barPct(b.count) }"
                  :title="`比例 ${b.ratio.toFixed(2)} · ${b.count} 张`"
                />
              </div>
              <div class="hist-axis"><span>竖构图</span><span>1:1</span><span>横构图</span></div>
            </div>

            </details>

            <div v-if="exportFmt === 'pdf'" class="estimate">
              <span>预计 PDF 体积</span>
              <b class="mono">{{ estimate ? fmtSize(estimate.bytes) : "—" }}</b>
              <p v-if="estimate?.is_original_sum" class="hint warn">
                原图无损档：JPEG/PNG 原始字节直塞，体积≈原图总和，这是预期行为。
              </p>
              <p v-else class="hint">{{ qualityNote }} · {{ dpiNote }}</p>
            </div>

            <div class="preview-note">
              <h3>下一步</h3>
              <p>生成预览会先排版，再把每页合成为整页预览图；预览和最终导出使用同一份坐标。</p>
            </div>
          </div>
        </aside>
      </div>

      <footer class="footer-bar">
        <div class="footer-info">
          <span v-if="scan">
            {{ keptCount }} 张<span v-if="keptCount !== scan.count" class="muted">（共 {{ scan.count }}）</span>
            → {{ pagesCount }} 页
          </span>
          <span class="footer-output">{{ exportLabel }} · {{ exportFmt === 'pdf' ? qualityNote : imageQualityLabel }} · {{ exportFmt === 'pdf' ? dpiNote : imageResolutionLabel }}</span>
        </div>
        <button class="btn primary big" :disabled="!canLayout" @click="startLayout">
          生成预览（先排版）
        </button>
      </footer>
    </main>

    <!-- ③ 排版中 -->
    <main v-else-if="phase === 'laying_out'" class="center-stage">
      <section class="task-card">
        <h2>正在排版</h2>
        <p class="task-sub">切分树 + 模拟退火，尽量填满每一页</p>
        <div class="progress-track big">
          <div class="progress-bar" :class="{ complete: (layoutProgress?.percent || 0) >= 100 }" :style="{ width: (layoutProgress?.percent || 0) + '%' }"></div>
        </div>
        <div class="progress-line">
          <span>{{ layoutProgress?.message || "准备排版…" }}</span>
          <span class="mono">{{ Math.round(layoutProgress?.percent || 0) }}%</span>
        </div>
        <div class="stat-row">
          <div><span>页数</span><b>{{ layoutProgress?.total || pagesCount }}</b></div>
          <div><span>已用时间</span><b>{{ fmtDuration(elapsedSec) }}</b></div>
        </div>
        <button class="btn danger" :disabled="cancelling" @click="cancelCurrent">
          {{ cancelling ? "正在取消…" : "取消排版" }}
        </button>
      </section>
    </main>

    <!-- ④ 预览 -->
    <main v-else-if="phase === 'preview_ready' && summary" class="preview-stage" ref="previewStage">
      <div class="preview-toolbar">
        <button class="btn ghost" @click="backToParams">← 返回参数</button>
        <div class="page-indicator mono">
          第 {{ currentPage + 1 }} / {{ summary.pages }} 页
        </div>
        <div class="toolbar-mid">
          <div class="segmented inline">
            <label :class="{ active: previewMode === 'scroll' }" @click="switchPreviewMode('scroll')">
              <span class="seg-title">上下滚动</span>
            </label>
            <label :class="{ active: previewMode === 'swipe' }" @click="switchPreviewMode('swipe')">
              <span class="seg-title">左右滑动</span>
            </label>
          </div>
          <button class="btn ghost" @click="toggleFullscreen">
            {{ previewFullscreen ? "退出全屏" : "全屏" }}
          </button>
        </div>
        <div class="export-summary"><b>{{ exportLabel }}</b><span>{{ exportFmt === 'pdf' ? qualityNote : imageQualityLabel }} · {{ exportFmt === 'pdf' ? dpiNote : imageResolutionLabel }}</span></div>
        <button class="btn primary" @click="startExport">
          {{ exportFmt === "pdf" ? "导出 PDF" : `导出 ${exportFmt.toUpperCase()}` }}
        </button>
      </div>

      <div v-if="!prewarmDone && !prewarmCancelled" class="prewarm-strip">
        <div class="prewarm-text">
          <span>预览生成中 {{ prewarmCurrent }} / {{ prewarmTotal }}</span>
          <span class="mono">{{ fmtDuration(previewElapsed) }}</span>
        </div>
        <div class="progress-track slim">
          <div
            class="progress-bar"
            :class="{ complete: (previewProgress?.percent || 0) >= 100 }"
            :style="{ width: (previewProgress?.percent || 0) + '%' }"
          ></div>
        </div>
        <button class="link" @click="cancelCurrent">取消预生成</button>
      </div>
      <div v-else-if="prewarmCancelled" class="prewarm-strip">
        <span>预生成已取消 · 滚动/翻页时按需生成</span>
        <button class="link" @click="startPrewarm">继续预生成</button>
      </div>
      <div v-else class="prewarm-strip done">
        <span>预览已全部生成 · 共 {{ summary.pages }} 页</span>
        <span class="mono">平均填充 {{ (summary.fill_avg * 100).toFixed(1) }}%</span>
      </div>

      <div v-if="previewMode === 'scroll'" class="preview-scroll" ref="scrollArea">
        <div
          v-for="(_, i) in summary.pages"
          :key="i"
          class="page-card"
          :data-page="i"
          :style="{ aspectRatio: pageAspect }"
        >
          <img v-if="previews[i]" :src="previews[i]" :alt="`第 ${i + 1} 页`" />
          <div v-else class="page-placeholder">
            <span>预览生成中…</span>
          </div>
          <span class="page-no mono">{{ i + 1 }}</span>
        </div>
      </div>

      <div
        v-else
        class="preview-swipe"
        @pointerdown="onSwipeStart"
        @pointerup="onSwipeEnd"
      >
        <button class="nav-btn left" @click="goPage(-1)" :disabled="currentPage === 0">‹</button>
        <div class="swipe-page" :style="{ aspectRatio: pageAspect }">
          <img
            v-if="previews[currentPage]"
            :src="previews[currentPage]"
            :alt="`第 ${currentPage + 1} 页`"
          />
          <div v-else class="page-placeholder"><span>预览生成中…</span></div>
        </div>
        <button
          class="nav-btn right"
          @click="goPage(1)"
          :disabled="currentPage >= summary.pages - 1"
        >›</button>
      </div>

      <p v-if="error" class="error floating" role="alert">{{ error }}</p>
    </main>

    <!-- ⑤ 导出中 -->
    <main v-else-if="phase === 'exporting'" class="center-stage">
      <section class="task-card">
        <h2>{{ exportFmt === 'pdf' ? '正在写入 PDF' : `正在导出 ${exportLabel} 图片` }}</h2>
        <p class="task-sub mono" :title="exportingPath">{{ exportingPath }}</p>
        <div class="progress-track big">
          <div class="progress-bar" :class="{ complete: (exportProgress?.percent || 0) >= 100 }" :style="{ width: (exportProgress?.percent || 0) + '%' }"></div>
        </div>
        <div class="progress-line">
          <span>{{ exportProgress?.message || "准备写入…" }}</span>
          <span class="mono">{{ Math.round(exportProgress?.percent || 0) }}%</span>
        </div>
        <div class="stat-row">
          <div>
            <span>页数</span>
            <b>{{ exportProgress?.pages_done || 0 }} / {{ exportProgress?.pages_total || summary?.pages || 0 }}</b>
          </div>
          <div>
            <span>已写</span>
            <b>{{ fmtSize(exportProgress?.bytes_written || 0) }}<template v-if="exportProgress?.bytes_total"> / {{ fmtSize(exportProgress.bytes_total) }}</template></b>
          </div>
          <div>
            <span>速度</span>
            <b class="mono">{{ fmtSpeed(exportProgress?.speed_bps || 0) }}</b>
          </div>
          <div><span>已用时间</span><b>{{ fmtDuration(exportElapsed) }}</b></div>
        </div>
        <p v-if="exportProgress?.current_file" class="hint mono">
          当前：{{ exportProgress.current_file }}
        </p>
        <button class="btn danger" :disabled="cancelling" @click="cancelCurrent">
          {{ cancelling ? "正在取消…" : "取消导出" }}
        </button>
      </section>
    </main>

    <!-- ⑥ 完成 -->
    <main v-else-if="phase === 'done' && exportResult" class="center-stage">
      <section class="task-card done-card">
        <div class="done-icon">✓</div>
        <h2>{{ exportResult.format === 'pdf' ? 'PDF 已生成' : `${exportResult.format === 'webp' ? 'WebP' : 'AVIF'} 图片已生成` }}</h2>
        <p class="task-sub mono" :title="resultPath">{{ resultPath }}</p>
        <div class="stat-row">
          <div><span>{{ exportResult.format === 'pdf' ? '页数' : '图片数' }}</span><b>{{ exportResult.pages }}</b></div>
          <div><span>体积</span><b>{{ fmtSize(exportResult.bytes) }}</b></div>
          <div><span>用时</span><b>{{ fmtDuration(exportResult.ms / 1000) }}</b></div>
          <div><span>平均速度</span><b class="mono">{{ fmtSpeed(exportResult.speed_bps) }}</b></div>
        </div>
        <p v-if="resultSkipped" class="hint warn">
          有 {{ resultSkipped }} 张图片无法解码，已跳过。
        </p>
        <div class="done-actions">
          <template v-if="exportResult.format === 'pdf'">
            <button class="btn primary" @click="openPdf">打开 PDF</button>
            <button class="btn" @click="revealPdf">打开所在文件夹</button>
          </template>
          <template v-else>
            <button class="btn primary" @click="openImageFolder">打开图片文件夹</button>
            <button class="btn" :disabled="copyingImages" @click="copyAllImages">{{ copyingImages ? '复制中…' : '复制所有图' }}</button>
          </template>
          <button class="btn ghost" @click="backToPreview">返回预览</button>
          <button class="btn ghost" @click="phase = 'ready'">调整参数</button>
          <button class="btn ghost" @click="chooseFolder">继续拼图</button>
        </div>
        <p v-if="copyMessage" class="hint" role="status">{{ copyMessage }}</p>
        <p v-if="error" class="error" role="alert">{{ error }}</p>
      </section>
    </main>
  </div>
  </div>
</template>
