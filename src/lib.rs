use wasm_bindgen::prelude::*;
use serde::{Serialize};
use std::ffi::CString;
use std::os::raw::c_char;

#[derive(Serialize)]
struct TimeRange {
    start: f64,
    end: f64,
}

// Memory management functions for JS to call
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[wasm_bindgen]
pub fn free_string(ptr: *mut c_char) {
    unsafe {
        if !ptr.is_null() {
            let _ = CString::from_raw(ptr);
        }
    }
}

/// A highly optimized function to find voiced segments in raw audio data.
/// It receives raw audio data from JS, analyzes it, and returns a JSON string
/// containing an array of {start, end} time ranges.
#[wasm_bindgen]
pub fn find_voiced_segments(
    audio_data_ptr: *const f32,
    data_len: usize,
    sample_rate: f64,
    threshold_db: f64,
    chunk_size_ms: f64,
    min_silence_duration_ms: f64,
    padding_ms: f64,
) -> *mut c_char {
    let audio_data = unsafe { std::slice::from_raw_parts(audio_data_ptr, data_len) };
    
    let chunk_size_samples = (chunk_size_ms / 1000.0 * sample_rate) as usize;
    if chunk_size_samples == 0 { 
        let result_str = CString::new("[]").unwrap();
        return result_str.into_raw();
    }
    
    let mut volumes = Vec::new();
    for chunk in audio_data.chunks(chunk_size_samples) {
        let sum_squares: f32 = chunk.iter().map(|&sample| sample * sample).sum();
        let rms = (sum_squares / chunk.len() as f32).sqrt();
        let dbfs = 20.0 * rms.log10() as f64;
        volumes.push(dbfs);
    }
    
    let mut silent_ranges = Vec::new();
    let mut current_silence_start: Option<usize> = None;

    for (i, &db) in volumes.iter().enumerate() {
        if db < threshold_db {
            if current_silence_start.is_none() {
                current_silence_start = Some(i);
            }
        } else {
            if let Some(start_index) = current_silence_start {
                silent_ranges.push((start_index, i));
            }
            current_silence_start = None;
        }
    }
    if let Some(start_index) = current_silence_start {
        silent_ranges.push((start_index, volumes.len()));
    }

    let min_silence_chunks = (min_silence_duration_ms / chunk_size_ms).ceil() as usize;
    let padding_chunks = (padding_ms / chunk_size_ms).floor() as usize;

    let mut merged_silent_ranges = Vec::new();
    for (start, end) in silent_ranges {
        let duration = end - start;
        if duration >= min_silence_chunks {
            let padded_start = start.saturating_sub(padding_chunks);
            let padded_end = (end + padding_chunks).min(volumes.len());
            
            if let Some(last) = merged_silent_ranges.last_mut() {
                if padded_start <= last.1 {
                    last.1 = padded_end;
                    continue;
                }
            }
            merged_silent_ranges.push((padded_start, padded_end));
        }
    }

    let mut voiced_segments = Vec::new();
    let mut last_end_chunk = 0;
    let total_chunks = volumes.len();
    let seconds_per_chunk = chunk_size_ms / 1000.0;
    
    for (start, end) in merged_silent_ranges {
        if start > last_end_chunk {
            voiced_segments.push(TimeRange {
                start: last_end_chunk as f64 * seconds_per_chunk,
                end: start as f64 * seconds_per_chunk,
            });
        }
        last_end_chunk = end;
    }

    if last_end_chunk < total_chunks {
         voiced_segments.push(TimeRange {
            start: last_end_chunk as f64 * seconds_per_chunk,
            end: total_chunks as f64 * seconds_per_chunk,
        });
    }

    let result_json = serde_json::to_string(&voiced_segments).unwrap_or_else(|_| "[]".to_string());
    let result_str = CString::new(result_json).unwrap();
    result_str.into_raw()
}
