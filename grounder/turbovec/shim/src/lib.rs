//! C-ABI shim over the turbovec crate (IdMapIndex: 2-4 bit quantized SIMD vector
//! search), so Rigger's Go grounder calls the real engine via cgo - no Python,
//! no reimplementation. The index owns u64 ids the caller maps back to chunks.

use turbovec::IdMapIndex;
use std::os::raw::c_int;

/// Create an index over dim-dimensional vectors quantized to bit_width (2-4).
/// Returns null on error (e.g. an invalid bit width).
#[no_mangle]
pub extern "C" fn rv_index_new(dim: usize, bit_width: usize) -> *mut IdMapIndex {
    match IdMapIndex::new(dim, bit_width) {
        Ok(idx) => Box::into_raw(Box::new(idx)),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Add n vectors (row-major, n*dim f32) with their u64 ids. Returns 0 on success.
///
/// # Safety
/// vectors must point to n*dim f32 and ids to n u64; idx must be a live index.
#[no_mangle]
pub unsafe extern "C" fn rv_index_add(
    idx: *mut IdMapIndex,
    vectors: *const f32,
    n: usize,
    dim: usize,
    ids: *const u64,
) -> c_int {
    if idx.is_null() || vectors.is_null() || ids.is_null() {
        return -1;
    }
    let index = &mut *idx;
    let vecs = std::slice::from_raw_parts(vectors, n * dim);
    let id_slice = std::slice::from_raw_parts(ids, n);
    match index.add_with_ids(vecs, id_slice) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Search for the k nearest ids to a single dim-length query, writing up to k
/// ids and scores to the caller's buffers. Returns the count written, or -1.
///
/// # Safety
/// query must point to dim f32; out_ids and out_scores to k slots each.
#[no_mangle]
pub unsafe extern "C" fn rv_index_search(
    idx: *const IdMapIndex,
    query: *const f32,
    dim: usize,
    k: usize,
    out_ids: *mut u64,
    out_scores: *mut f32,
) -> c_int {
    if idx.is_null() || query.is_null() || out_ids.is_null() || out_scores.is_null() {
        return -1;
    }
    let index = &*idx;
    let q = std::slice::from_raw_parts(query, dim);
    let (scores, ids) = index.search(q, k);
    let count = ids.len().min(k);
    for i in 0..count {
        *out_ids.add(i) = ids[i];
        *out_scores.add(i) = scores[i];
    }
    count as c_int
}

/// Free an index returned by rv_index_new.
///
/// # Safety
/// idx must have come from rv_index_new and not been freed.
#[no_mangle]
pub unsafe extern "C" fn rv_index_free(idx: *mut IdMapIndex) {
    if !idx.is_null() {
        drop(Box::from_raw(idx));
    }
}
