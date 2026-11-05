export type PageKind = "a4p" | "a4l" | "a3p" | "a3l";
export type Mode = "cover" | "fit";
export type Quality = "lossless" | "high" | "standard" | "small";
export type Phase =
  | "idle"
  | "importing"
  | "ready"
  | "laying_out"
  | "preview_ready"
  | "exporting"
  | "done";
export type PreviewMode = "scroll" | "swipe";

export interface ScanProgress {
  scanned: number;
  total: number;
  usable: number;
  bytes: number;
  percent: number;
  elapsed_ms: number;
  message: string;
}

export interface ScanInfo {
  files: number;
  count: number;
  skipped: number;
  total_bytes: number;
  avg_bytes: number;
  landscape: number;
  portrait: number;
  square: number;
  ms_scan: number;
  histogram?: { ratio: number; count: number }[];
}

export type LayoutMode = "grid" | "mosaic";

export interface LayoutOptions {
  page: PageKind;
  layout: LayoutMode;
  margin_mm: number;
  gutter_mm: number;
  per_page: number;
  per_page_plan?: number[] | null;
  /** 相近比例优先排布（仅经典模式） */
  cluster?: boolean;
  min_w?: number | null;
  min_h?: number | null;
  ar_min?: number | null;
  ar_max?: number | null;
  pages: number | null;
  mode: Mode;
  quality: Quality;
  dpi: number | null;
  seed: number;
  iters: number;
  even: boolean;
}

export interface LayoutProgress {
  current: number;
  total: number;
  percent: number;
  elapsed_ms: number;
  message: string;
}

export interface LayoutSummary {
  images: number;
  pages: number;
  per_page: number;
  fill_avg: number;
  page_w_mm: number;
  page_h_mm: number;
  ms_scan: number;
  ms_layout: number;
  total_bytes: number;
}

export interface PreviewProgress {
  current: number;
  total: number;
  percent: number;
  elapsed_ms: number;
  message: string;
}

export interface PreviewImage {
  page: number;
  width: number;
  height: number;
  path: string;
  data_url: string;
  cached: boolean;
}

export interface ExportProgress {
  pages_done: number;
  pages_total: number;
  bytes_written: number;
  bytes_total: number;
  speed_bps: number;
  percent: number;
  elapsed_ms: number;
  current_file: string | null;
  message: string;
}

export interface ExportResult {
  pages: number;
  bytes: number;
  ms: number;
  speed_bps: number;
  skipped: number;
  pdf_path: string;
}

export type ImageFormat = "webp" | "avif";

export interface ImageExportResult {
  files: string[];
  out_dir: string;
  bytes: number;
  pages: number;
  ms: number;
  speed_bps: number;
}

export type CompletedExport =
  | (ExportResult & { format: "pdf" })
  | (ImageExportResult & { format: ImageFormat });

export interface Estimate {
  bytes: number;
  pages: number;
  images: number;
  is_original_sum: boolean;
}

export interface PlanOption {
  pages: number;
  plan: number[];
  fill: number;
  spread: number;
  min: number;
  max: number;
  relaxed?: boolean;
}

export interface Recommendation {
  plan: number[];
  plan_fill: number;
  mid: number;
  lo: number;
  hi: number;
  pages: number;
  options?: PlanOption[];
}

export interface FilterStats {
  total: number;
  kept: number;
}

export interface Prefs {
  export_format?: "pdf" | ImageFormat;
  image_dpi?: number | null;
  image_quality?: number;
  page: PageKind;
  layout?: LayoutMode;
  margin_mm: number;
  gutter_mm: number;
  mode: Mode;
  per_page: number | null;
  quality: Quality;
  dpi: number | null;
  preview_mode: PreviewMode;
  min_w?: number | null;
  min_h?: number | null;
  ar_min?: number | null;
  ar_max?: number | null;
}

export interface PathsInfo {
  exe_dir: string;
  data_dir: string;
  config_path: string;
  cache_dir: string;
  webview_dir: string;
  portable: boolean;
}
