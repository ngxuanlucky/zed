/// Unit tests targeting issues identified in the rope/chunk code review.
///
/// Issues covered:
/// 1. `Chunk::floor_char_boundary` correctness (byte-loop vs bitmap consistency)
/// 2. `ChunkSlice` → `Chunk` conversion preserves all fields
/// 3. `Rope` coordinate round-trips (offset ↔ point ↔ utf16)
/// 4. Empty rope edge cases
/// 5. `Rope::floor_char_boundary` / `ceil_char_boundary` on multibyte chars
#[cfg(test)]
mod tests {
    use super::*;
    use Bias::{Left, Right};

    // ── Chunk::floor_char_boundary ────────────────────────────────────────────

    /// Verifies that `floor_char_boundary` returns the same result as the
    /// standard library's `str::floor_char_boundary` for every byte index
    /// inside a multibyte string.
    #[test]
    fn test_chunk_floor_char_boundary_matches_stdlib() {
        // "日本語" — each char is 3 bytes (9 bytes total)
        let text = "日本語";
        let chunk = Chunk::new(text);
        for i in 0..=text.len() {
            assert_eq!(
                chunk.floor_char_boundary(i),
                text.floor_char_boundary(i),
                "floor_char_boundary mismatch at byte index {i}"
            );
        }
    }

    /// Out-of-bounds index should clamp to `text.len()`.
    #[test]
    fn test_chunk_floor_char_boundary_oob() {
        let text = "abc";
        let chunk = Chunk::new(text);
        assert_eq!(chunk.floor_char_boundary(100), text.len());
    }

    /// Index exactly at a char boundary should be returned unchanged.
    #[test]
    fn test_chunk_floor_char_boundary_at_boundary() {
        let text = "a🏀b"; // 'a'=1, '🏀'=4, 'b'=1 → boundaries at 0,1,5,6
        let chunk = Chunk::new(text);
        assert_eq!(chunk.floor_char_boundary(0), 0);
        assert_eq!(chunk.floor_char_boundary(1), 1);
        assert_eq!(chunk.floor_char_boundary(5), 5);
        assert_eq!(chunk.floor_char_boundary(6), 6);
    }

    /// Index inside a multibyte char should floor to the char's start.
    #[test]
    fn test_chunk_floor_char_boundary_inside_multibyte() {
        let text = "a🏀b";
        let chunk = Chunk::new(text);
        // bytes 2, 3, 4 are inside '🏀' which starts at byte 1
        assert_eq!(chunk.floor_char_boundary(2), 1);
        assert_eq!(chunk.floor_char_boundary(3), 1);
        assert_eq!(chunk.floor_char_boundary(4), 1);
    }

    // ── ChunkSlice → Chunk conversion ────────────────────────────────────────

    /// Converting a `ChunkSlice` to a `Chunk` must preserve text content.
    #[test]
    fn test_chunk_slice_into_chunk_preserves_text() {
        let text = "hello 🌍";
        let chunk = Chunk::new(text);
        let slice = chunk.as_slice();
        let owned: Chunk = slice.into();
        assert_eq!(&*owned.text, text);
    }

    /// A sub-slice converted to `Chunk` must have correct bitmap fields.
    #[test]
    fn test_chunk_slice_into_chunk_preserves_bitmaps() {
        let text = "ab\ncd";
        let chunk = Chunk::new(text);
        // slice the whole thing and round-trip
        let owned: Chunk = chunk.as_slice().into();
        assert_eq!(owned.chars(), chunk.chars());
        assert_eq!(owned.newlines(), chunk.newlines());
        assert_eq!(owned.tabs(), chunk.tabs());
    }

    // ── Rope: empty rope edge cases ───────────────────────────────────────────

    #[test]
    fn test_empty_rope_len_and_point() {
        let rope = Rope::new();
        assert_eq!(rope.len(), 0);
        assert!(rope.is_empty());
        assert_eq!(rope.max_point(), Point::new(0, 0));
        assert_eq!(rope.offset_to_point(0), Point::new(0, 0));
    }

    #[test]
    fn test_empty_rope_char_boundary() {
        let rope = Rope::new();
        assert!(rope.is_char_boundary(0));
        assert!(!rope.is_char_boundary(1));
    }

    #[test]
    fn test_empty_rope_floor_ceil_char_boundary() {
        let rope = Rope::new();
        assert_eq!(rope.floor_char_boundary(0), 0);
        assert_eq!(rope.ceil_char_boundary(0), 0);
    }

    // ── Rope: coordinate round-trips ──────────────────────────────────────────

    /// offset → point → offset must be identity for ASCII.
    #[test]
    fn test_rope_offset_point_roundtrip_ascii() {
        let rope = Rope::from("abc\ndef\nghi");
        for offset in 0..=rope.len() {
            let point = rope.offset_to_point(offset);
            assert_eq!(
                rope.point_to_offset(point),
                offset,
                "round-trip failed at offset {offset}"
            );
        }
    }

    /// offset → point → offset must be identity for multibyte chars.
    #[test]
    fn test_rope_offset_point_roundtrip_multibyte() {
        let text = "日\n本\n語";
        let rope = Rope::from(text);
        for (offset, _) in text.char_indices().chain(Some((text.len(), '\0'))) {
            let point = rope.offset_to_point(offset);
            assert_eq!(
                rope.point_to_offset(point),
                offset,
                "round-trip failed at offset {offset}"
            );
        }
    }

    /// offset → utf16 → offset must be identity for surrogate-pair chars.
    #[test]
    fn test_rope_offset_utf16_roundtrip() {
        // '𝄞' (U+1D11E) is a surrogate pair in UTF-16 (2 code units, 4 bytes UTF-8)
        let text = "a𝄞b";
        let rope = Rope::from(text);
        for (offset, _) in text.char_indices().chain(Some((text.len(), '\0'))) {
            let utf16 = rope.offset_to_offset_utf16(offset);
            assert_eq!(
                rope.offset_utf16_to_offset(utf16),
                offset,
                "utf16 round-trip failed at offset {offset}"
            );
        }
    }

    // ── Rope: floor/ceil char boundary ────────────────────────────────────────

    #[test]
    fn test_rope_floor_char_boundary_multibyte() {
        // "a🏀b": 'a'=byte 0, '🏀'=bytes 1-4, 'b'=byte 5
        let rope = Rope::from("a🏀b");
        assert_eq!(rope.floor_char_boundary(0), 0);
        assert_eq!(rope.floor_char_boundary(1), 1);
        assert_eq!(rope.floor_char_boundary(2), 1); // inside '🏀'
        assert_eq!(rope.floor_char_boundary(4), 1); // inside '🏀'
        assert_eq!(rope.floor_char_boundary(5), 5);
        assert_eq!(rope.floor_char_boundary(6), 6);
    }

    #[test]
    fn test_rope_ceil_char_boundary_multibyte() {
        let rope = Rope::from("a🏀b");
        assert_eq!(rope.ceil_char_boundary(1), 1);
        assert_eq!(rope.ceil_char_boundary(2), 5); // ceil inside '🏀' → end of '🏀'
        assert_eq!(rope.ceil_char_boundary(5), 5);
    }

    // ── Rope: line_len ────────────────────────────────────────────────────────

    #[test]
    fn test_rope_line_len() {
        let rope = Rope::from("abc\nde\nf");
        assert_eq!(rope.line_len(0), 3);
        assert_eq!(rope.line_len(1), 2);
        assert_eq!(rope.line_len(2), 1);
    }

    #[test]
    fn test_rope_line_len_empty_lines() {
        let rope = Rope::from("\n\n");
        assert_eq!(rope.line_len(0), 0);
        assert_eq!(rope.line_len(1), 0);
        assert_eq!(rope.line_len(2), 0);
    }

    // ── Rope: clip_offset ─────────────────────────────────────────────────────

    /// Clipping an offset inside a multibyte char should snap to the correct
    /// boundary depending on bias.
    #[test]
    fn test_rope_clip_offset_multibyte() {
        // '🧘' is 4 bytes
        let rope = Rope::from("🧘");
        assert_eq!(rope.clip_offset(2, Left), 0);
        assert_eq!(rope.clip_offset(2, Right), 4);
    }

    // ── Rope: text() helper ───────────────────────────────────────────────────

    #[test]
    fn test_rope_text_roundtrip() {
        let text = "Hello, 世界!\nSecond line.";
        let rope = Rope::from(text);
        assert_eq!(rope.chunks().collect::<String>(), text);
    }
}
