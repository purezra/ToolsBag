use crate::converter;
use crate::detector;
use crate::models::{
    AnalyzeRequest, AnalyzeResult, Convert3Request, Convert3Result, ConvertRequest, ConvertResult,
    EpubConvertRequest, EpubConvertResult, FileEntry, RepairRequest, RepairResult,
};
use crate::repairer;
use crate::tools::image_batch;
use std::path::PathBuf;
use tauri::AppHandle;

#[tauri::command]
pub fn analyze_folder(app: AppHandle, req: AnalyzeRequest) -> Result<AnalyzeResult, String> {
    detector::analyze(app, req)
}

#[tauri::command]
pub fn repair_images(app: AppHandle, req: RepairRequest) -> Result<RepairResult, String> {
    repairer::repair(app, req)
}

#[tauri::command]
pub fn convert_to_pdf(app: AppHandle, req: ConvertRequest) -> Result<ConvertResult, String> {
    converter::convert(app, req)
}

#[tauri::command]
pub fn list_images(input_dir: PathBuf, recursive: bool) -> Result<Vec<FileEntry>, String> {
    image_batch::list_images(input_dir, recursive)
}

#[tauri::command]
pub fn convert_tool3(app: AppHandle, req: Convert3Request) -> Result<Convert3Result, String> {
    image_batch::convert(app, req)
}

#[tauri::command]
pub fn convert_to_epub(
    app: AppHandle,
    req: EpubConvertRequest,
) -> Result<EpubConvertResult, String> {
    image_batch::convert_to_epub(app, req)
}
