//! 流式 PDF 写入器。
//!
//! 关键点：**图片字节不驻留内存**。每张图准备完成后立刻写进文件，
//! 记录对象偏移；xref 表和 trailer 最后追加。峰值内存只和流水线里
//! 正在处理的少数几张图有关，与图片总数无关。
//!
//! 对象编号约定：1 = Pages 树，2 = Catalog（最后回填），其余对象边写边分配。

use crate::encode::PreparedImage;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

pub const MM_TO_PT: f32 = 72.0 / 25.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// 裁切铺满
    Cover,
    /// 完整贴合（留白）
    Fit,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Cover
    }
}

/// 一页里的一个图片位置（毫米，左上角原点）
#[derive(Clone, Debug)]
pub struct PageCell {
    pub name: String,
    /// 格子矩形（用于裁切）
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// 图片实际绘制矩形（cover 时可能超出格子）
    pub dx: f32,
    pub dy: f32,
    pub dw: f32,
    pub dh: f32,
}

#[derive(Clone, Debug)]
pub struct ImageRef {
    pub name: String,
    pub id: u32,
    pub width: u32,
    pub height: u32,
}

pub struct PdfWriter {
    file: Option<BufWriter<File>>,
    pos: u64,
    offsets: BTreeMap<u32, u64>,
    next_id: u32,
    pages: Vec<u32>,
    page_w_mm: f32,
    page_h_mm: f32,
    tmp_path: PathBuf,
    final_path: PathBuf,
    finished: bool,
}

impl PdfWriter {
    pub fn new(out: &Path, page_w_mm: f32, page_h_mm: f32) -> std::io::Result<Self> {
        if let Some(parent) = out.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let file_name = out
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "tilepress.pdf".to_string());
        let tmp_path = out.with_file_name(format!("{}.part", file_name));
        let f = File::create(&tmp_path)?;
        let mut w = PdfWriter {
            file: Some(BufWriter::with_capacity(1024 * 1024, f)),
            pos: 0,
            offsets: BTreeMap::new(),
            next_id: 3,
            pages: Vec::new(),
            page_w_mm,
            page_h_mm,
            tmp_path,
            final_path: out.to_path_buf(),
            finished: false,
        };
        w.write_raw(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n")?;
        Ok(w)
    }

    fn write_raw(&mut self, b: &[u8]) -> std::io::Result<()> {
        let f = self
            .file
            .as_mut()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "PDF 已关闭"))?;
        f.write_all(b)?;
        self.pos += b.len() as u64;
        Ok(())
    }

    fn begin_obj(&mut self) -> std::io::Result<u32> {
        let id = self.next_id;
        self.next_id += 1;
        self.offsets.insert(id, self.pos);
        self.write_raw(format!("{} 0 obj\n", id).as_bytes())?;
        Ok(id)
    }

    fn end_obj(&mut self) -> std::io::Result<()> {
        self.write_raw(b"\nendobj\n")
    }

    fn write_reserved(&mut self, id: u32, body: &[u8]) -> std::io::Result<()> {
        self.offsets.insert(id, self.pos);
        self.write_raw(format!("{} 0 obj\n", id).as_bytes())?;
        self.write_raw(body)?;
        self.write_raw(b"\nendobj\n")
    }

    /// 立刻把一张图写进 PDF（必要时先写 SMask），返回引用信息。
    pub fn write_image(&mut self, img: &PreparedImage) -> std::io::Result<ImageRef> {
        let smask_id = if let Some(sm) = img.smask.as_ref() {
            let id = self.begin_obj()?;
            self.write_raw(
                format!(
                    "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
                    sm.width,
                    sm.height,
                    sm.data.len()
                )
                .as_bytes(),
            )?;
            self.write_raw(&sm.data)?;
            self.write_raw(b"\nendstream")?;
            self.end_obj()?;
            Some(id)
        } else {
            None
        };

        let id = self.begin_obj()?;
        let dict = img.dict();
        let smask = smask_id
            .map(|i| format!(" /SMask {} 0 R", i))
            .unwrap_or_default();
        self.write_raw(
            format!(
                "<< {} /Length {}{} >>\nstream\n",
                dict,
                img.data.len(),
                smask
            )
            .as_bytes(),
        )?;
        self.write_raw(&img.data)?;
        self.write_raw(b"\nendstream")?;
        self.end_obj()?;

        let name = image_name(id);
        Ok(ImageRef {
            name,
            id,
            width: img.width,
            height: img.height,
        })
    }

    pub fn write_page(&mut self, cells: &[PageCell]) -> std::io::Result<()> {
        let mut content = String::new();
        let mut used: Vec<(String, u32)> = Vec::new();

        for c in cells {
            let x = c.x * MM_TO_PT;
            let w = c.w * MM_TO_PT;
            let h = c.h * MM_TO_PT;
            let y = (self.page_h_mm - c.y - c.h) * MM_TO_PT;
            let dx = c.dx * MM_TO_PT;
            let dy = (self.page_h_mm - c.dy - c.dh) * MM_TO_PT;
            let dw = c.dw * MM_TO_PT;
            let dh = c.dh * MM_TO_PT;
            content.push_str(&format!(
                "q\n{:.3} {:.3} {:.3} {:.3} re W n\n{:.3} 0 0 {:.3} {:.3} {:.3} cm\n/{} Do\nQ\n",
                x, y, w, h, dw, dh, dx, dy, c.name
            ));
        }

        for c in cells {
            if !used.iter().any(|(n, _)| *n == c.name) {
                used.push((c.name.clone(), name_to_id(&c.name)));
            }
        }

        let mut xo = String::new();
        for (n, id) in &used {
            xo.push_str(&format!("/{} {} 0 R ", n, id));
        }
        let cid = self.begin_obj()?;
        let stream = content.into_bytes();
        self.write_raw(format!("<< /Length {} >>\nstream\n", stream.len()).as_bytes())?;
        self.write_raw(&stream)?;
        self.write_raw(b"\nendstream")?;
        self.end_obj()?;

        let page_body = format!(
            "<< /Type /Page /Parent 1 0 R /MediaBox [0 0 {:.3} {:.3}] /Resources << /XObject << {}>> >> /Contents {} 0 R >>",
            self.page_w_mm * MM_TO_PT,
            self.page_h_mm * MM_TO_PT,
            xo,
            cid
        );
        let pid = self.begin_obj()?;
        self.write_raw(page_body.as_bytes())?;
        self.end_obj()?;
        self.pages.push(pid);
        Ok(())
    }

    /// 写 Pages 树 / Catalog / xref，落盘并原子改名。
    pub fn finish(mut self) -> std::io::Result<u64> {
        let kids: Vec<String> = self.pages.iter().map(|p| format!("{} 0 R", p)).collect();
        let pages_obj = format!(
            "<< /Type /Pages /Count {} /Kids [{}] >>",
            self.pages.len(),
            kids.join(" ")
        );
        self.write_reserved(1, pages_obj.as_bytes())?;
        self.write_reserved(2, b"<< /Type /Catalog /Pages 1 0 R >>")?;

        let max_id = self.next_id.saturating_sub(1);
        let xref_pos = self.pos;
        let mut xref = String::new();
        xref.push_str(&format!("xref\n0 {}\n", max_id + 1));
        xref.push_str("0000000000 65535 f \n");
        for id in 1..=max_id {
            match self.offsets.get(&id) {
                Some(off) => xref.push_str(&format!("{:010} 00000 n \n", off)),
                None => xref.push_str("0000000000 65535 f \n"),
            }
        }
        xref.push_str(&format!(
            "trailer\n<< /Size {} /Root 2 0 R >>\nstartxref\n{}\n%%EOF\n",
            max_id + 1,
            xref_pos
        ));
        self.write_raw(xref.as_bytes())?;

        if let Some(mut f) = self.file.take() {
            f.flush()?;
        }
        if self.final_path.exists() {
            let _ = fs::remove_file(&self.final_path);
        }
        fs::rename(&self.tmp_path, &self.final_path)?;
        self.finished = true;
        Ok(fs::metadata(&self.final_path).map(|m| m.len()).unwrap_or(0))
    }

    /// 取消：关闭并删除残缺文件。
    pub fn abort(mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.tmp_path);
        self.finished = true;
    }
}

impl Drop for PdfWriter {
    fn drop(&mut self) {
        if !self.finished {
            self.file.take();
            let _ = fs::remove_file(&self.tmp_path);
        }
    }
}

/// `write_image` 返回的名字形如 `Im0`，这里把 id 编进名字，避免额外映射。
/// 约定：`name = "Im{id}"`。调用方必须用 [`image_name`] 构造名字。
pub fn image_name(id: u32) -> String {
    format!("Im{}", id)
}

fn name_to_id(name: &str) -> u32 {
    name.trim_start_matches("Im").parse().unwrap_or(0)
}
