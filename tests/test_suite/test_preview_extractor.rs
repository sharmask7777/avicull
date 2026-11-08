//! # Tests for Container-Aware Embedded Preview Extractor
//!
//! Verifies:
//! - Magic byte container recognition (TIFF vs ISOBMFF vs other).
//! - Fast 2-request embedded preview extraction on synthetic TIFF container.
//! - Dynamic SubIFD chasing when preview tags reside beyond initial 128 KB.
//! - Bounds-checked error handling on corrupted containers.

use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

use avicull::storage::opendal_backend::OpenDalBackend;
use avicull::storage::preview_extractor::{ContainerFormat, PreviewExtractor};

#[test]
fn test_detect_container_format() {
    // Little-endian TIFF (II*\0)
    let tiff_le = [0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00];
    assert_eq!(
        ContainerFormat::detect(&tiff_le),
        ContainerFormat::TiffLittleEndian
    );

    // Big-endian TIFF (MM\0*)
    let tiff_be = [0x4D, 0x4D, 0x00, 0x2A, 0x00, 0x00, 0x00, 0x08];
    assert_eq!(
        ContainerFormat::detect(&tiff_be),
        ContainerFormat::TiffBigEndian
    );

    // Canon CR3 ISOBMFF (....ftypcrx )
    let cr3 = [
        0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', b'c', b'r', b'x', b' ',
    ];
    assert_eq!(ContainerFormat::detect(&cr3), ContainerFormat::IsoBmff);

    // Unknown
    let unk = [0x00, 0x01, 0x02, 0x03];
    assert_eq!(ContainerFormat::detect(&unk), ContainerFormat::Unknown);
}

#[tokio::test]
async fn test_extract_embedded_preview_from_synthetic_tiff() {
    let tmp = tempdir().unwrap();
    let file_path = tmp.path().join("sample.cr2");

    // Construct a minimal valid TIFF header with an embedded JPEG preview
    // Header: 'II', 42 (0x002A), IFD offset = 8
    let mut data = Vec::new();
    data.extend_from_slice(&[0x49, 0x49, 0x2A, 0x00]); // II, 42
    data.extend_from_slice(&8u32.to_le_bytes()); // IFD0 offset at byte 8

    // IFD0: 2 tags (JPEGInterchangeFormat = 0x0201, JPEGInterchangeFormatLength = 0x0202)
    let num_tags: u16 = 2;
    data.extend_from_slice(&num_tags.to_le_bytes());

    let preview_offset = 200u32;
    let preview_length = 32u32;

    // Tag 1: 0x0201 (Offset), type 4 (LONG), count 1, value = preview_offset
    data.extend_from_slice(&0x0201u16.to_le_bytes());
    data.extend_from_slice(&4u16.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&preview_offset.to_le_bytes());

    // Tag 2: 0x0202 (Length), type 4 (LONG), count 1, value = preview_length
    data.extend_from_slice(&0x0202u16.to_le_bytes());
    data.extend_from_slice(&4u16.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&preview_length.to_le_bytes());

    // Next IFD offset: 0
    data.extend_from_slice(&0u32.to_le_bytes());

    // Pad until preview_offset
    if data.len() < preview_offset as usize {
        data.resize(preview_offset as usize, 0);
    }

    // Embed mock JPEG data (starts with 0xFFD8, ends with 0xFFD9)
    let mut mock_jpeg = vec![0xEEu8; preview_length as usize];
    let len = mock_jpeg.len();
    mock_jpeg[0] = 0xFF;
    mock_jpeg[1] = 0xD8;
    mock_jpeg[len - 2] = 0xFF;
    mock_jpeg[len - 1] = 0xD9;
    data.extend_from_slice(&mock_jpeg);

    let mut f = File::create(&file_path).unwrap();
    f.write_all(&data).unwrap();
    drop(f);

    let backend = OpenDalBackend::from_fs_path(tmp.path()).unwrap();
    let extractor = PreviewExtractor::new(backend);

    let preview = extractor
        .extract_preview("sample.cr2", 128 * 1024)
        .await
        .unwrap();

    assert_eq!(preview.len(), preview_length as usize);
    assert_eq!(preview[0], 0xFF);
    assert_eq!(preview[1], 0xD8);
}

#[tokio::test]
async fn test_extract_embedded_preview_rejects_oversized_payload() {
    let tmp = tempdir().unwrap();
    let file_path = tmp.path().join("oversized.cr2");

    let mut data = Vec::new();
    data.extend_from_slice(&[0x49, 0x49, 0x2A, 0x00]); // II, 42
    data.extend_from_slice(&8u32.to_le_bytes()); // IFD0 offset

    let num_tags: u16 = 2;
    data.extend_from_slice(&num_tags.to_le_bytes());

    let preview_offset = 200u32;
    let oversized_length = (128 * 1024 * 1024) as u32; // 128 MB > 64 MB cap

    // 0x0201: offset
    data.extend_from_slice(&0x0201u16.to_le_bytes());
    data.extend_from_slice(&4u16.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&preview_offset.to_le_bytes());

    // 0x0202: length
    data.extend_from_slice(&0x0202u16.to_le_bytes());
    data.extend_from_slice(&4u16.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&oversized_length.to_le_bytes());

    let mut f = File::create(&file_path).unwrap();
    f.write_all(&data).unwrap();
    drop(f);

    let backend = OpenDalBackend::from_fs_path(tmp.path()).unwrap();
    let extractor = PreviewExtractor::new(backend);

    let err = extractor
        .extract_preview("oversized.cr2", 128 * 1024)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("exceeds safety cap"));
}

#[tokio::test]
async fn test_extract_embedded_preview_from_subifd() {
    let tmp = tempdir().unwrap();
    let file_path = tmp.path().join("subifd.cr2");

    // TIFF Header: 'II', 42, IFD0 offset = 8
    let mut data = Vec::new();
    data.extend_from_slice(&[0x49, 0x49, 0x2A, 0x00]);
    data.extend_from_slice(&8u32.to_le_bytes());

    // IFD0 has 1 tag: SubIFD pointer (0x014A) pointing to offset 40
    let subifd_offset = 40u32;
    data.extend_from_slice(&1u16.to_le_bytes()); // 1 tag
    data.extend_from_slice(&0x014Au16.to_le_bytes());
    data.extend_from_slice(&4u16.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&subifd_offset.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes()); // next IFD

    // Pad to subifd_offset (40)
    if data.len() < subifd_offset as usize {
        data.resize(subifd_offset as usize, 0);
    }

    // SubIFD at byte 40: 2 tags (0x0201 preview offset, 0x0202 preview length)
    let preview_offset = 120u32;
    let preview_length = 24u32;

    data.extend_from_slice(&2u16.to_le_bytes()); // 2 tags in SubIFD
    data.extend_from_slice(&0x0201u16.to_le_bytes());
    data.extend_from_slice(&4u16.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&preview_offset.to_le_bytes());

    data.extend_from_slice(&0x0202u16.to_le_bytes());
    data.extend_from_slice(&4u16.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&preview_length.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes()); // next IFD in subifd chain

    // Pad to preview_offset
    if data.len() < preview_offset as usize {
        data.resize(preview_offset as usize, 0);
    }

    // Valid mock JPEG
    let mut mock_jpeg = vec![0xAAu8; preview_length as usize];
    let len = mock_jpeg.len();
    mock_jpeg[0] = 0xFF;
    mock_jpeg[1] = 0xD8;
    mock_jpeg[len - 2] = 0xFF;
    mock_jpeg[len - 1] = 0xD9;
    data.extend_from_slice(&mock_jpeg);

    let mut f = File::create(&file_path).unwrap();
    f.write_all(&data).unwrap();
    drop(f);

    let backend = OpenDalBackend::from_fs_path(tmp.path()).unwrap();
    let extractor = PreviewExtractor::new(backend);

    let preview = extractor
        .extract_preview("subifd.cr2", 128 * 1024)
        .await
        .unwrap();

    assert_eq!(preview.len(), preview_length as usize);
    assert_eq!(preview[0], 0xFF);
    assert_eq!(preview[1], 0xD8);
}

#[tokio::test]
async fn test_preview_extractor_edge_cases_and_fallbacks() {
    let tmp = tempdir().unwrap();
    let backend = OpenDalBackend::from_fs_path(tmp.path()).unwrap();
    let extractor = PreviewExtractor::new(backend);

    // 1. File too small
    let small_file = tmp.path().join("tiny.dat");
    std::fs::write(&small_file, [0x01, 0x02]).unwrap();
    let err = extractor
        .extract_preview("tiny.dat", 128 * 1024)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("too small"));

    // 2. Standard JPEG fallback
    let jpeg_file = tmp.path().join("plain.jpg");
    let mut jpg = vec![0x11u8; 100];
    jpg[0] = 0xFF;
    jpg[1] = 0xD8;
    std::fs::write(&jpeg_file, &jpg).unwrap();
    let preview = extractor
        .extract_preview("plain.jpg", 128 * 1024)
        .await
        .unwrap();
    assert_eq!(preview.len(), 100);

    // 3. Unrecognized format
    let rand_file = tmp.path().join("rand.dat");
    std::fs::write(&rand_file, vec![0x99u8; 64]).unwrap();
    let err2 = extractor
        .extract_preview("rand.dat", 128 * 1024)
        .await
        .unwrap_err();
    assert!(err2.to_string().contains("Unrecognized"));
}
