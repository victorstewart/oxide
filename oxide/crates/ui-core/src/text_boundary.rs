use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[inline]
pub(crate) fn cluster_count(input: &str) -> usize {
    if input.is_ascii() {
        input.len()
    } else {
        UnicodeSegmentation::graphemes(input, true).count()
    }
}

#[inline]
pub(crate) fn byte_index_for_cluster(input: &str, cluster_index: usize) -> usize {
    if cluster_index == 0 {
        return 0;
    }
    if input.is_ascii() {
        return cluster_index.min(input.len());
    }
    UnicodeSegmentation::grapheme_indices(input, true)
        .nth(cluster_index)
        .map(|(idx, _)| idx)
        .unwrap_or(input.len())
}

#[inline]
pub(crate) fn cluster_range_to_byte(input: &str, range: Range<usize>) -> Range<usize> {
    if input.is_ascii() {
        return range.start.min(input.len())..range.end.min(input.len());
    }
    byte_index_for_cluster(input, range.start)..byte_index_for_cluster(input, range.end)
}

#[inline]
pub(crate) fn cluster_slice(input: &str, range: Range<usize>) -> String {
    let bytes = cluster_range_to_byte(input, range);
    input[bytes].to_owned()
}

pub(crate) fn cluster_boundaries(input: &str) -> Vec<usize> {
    if input.is_ascii() {
        let mut boundaries = Vec::with_capacity(input.len() + 1);
        for index in 0..=input.len() {
            boundaries.push(index);
        }
        return boundaries;
    }
    let mut boundaries = Vec::with_capacity(cluster_count(input) + 1);
    for (idx, _) in UnicodeSegmentation::grapheme_indices(input, true) {
        boundaries.push(idx);
    }
    boundaries.push(input.len());
    boundaries
}
