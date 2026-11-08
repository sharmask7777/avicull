//! # Container-Aware Embedded Preview Extractor
//!
//! Extracts embedded JPEG previews from camera RAW files (Canon CR2/CR3, Nikon NEF, Sony ARW, DNG)
//! over remote byte-range streams with minimal network roundtrips.
//!
//! ## Architectural Invariants
//! - **Two-Request Bound**: Header range (first ~128 KB) followed by exact preview byte range.
//! - **Bounded Chasing**: Dynamic IFD tag chasing capped at 3 requests to prevent runaway latency.
//! - **Zero Buffer Overflow**: Strict bounds checking on all tag offsets and sizes.

use super::opendal_backend::RemoteObjectBackend;

/// Supported raw container format types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerFormat {
    /// TIFF Little-Endian (`II*\0` - Canon CR2, Nikon NEF, Sony ARW, DNG).
    TiffLittleEndian,
    /// TIFF Big-Endian (`MM\0*`).
    TiffBigEndian,
    /// ISO Base Media File Format (Canon CR3).
    IsoBmff,
    /// Unknown or unsupported container format.
    Unknown,
}

impl ContainerFormat {
    /// Identifies the container format from the initial bytes of an asset.
    #[must_use]
    pub fn detect(header: &[u8]) -> Self {
        if header.len() >= 4 {
            if header[0..4] == [0x49, 0x49, 0x2A, 0x00] {
                return Self::TiffLittleEndian;
            }
            if header[0..4] == [0x4D, 0x4D, 0x00, 0x2A] {
                return Self::TiffBigEndian;
            }
        }
        if header.len() >= 12 && &header[4..8] == b"ftyp" {
            return Self::IsoBmff;
        }
        Self::Unknown
    }
}

/// Maximum allowed embedded preview size in bytes (64 MB).
/// Prevents OOM crashes from malicious or corrupt RAW file tags.
pub const MAX_PREVIEW_SIZE_BYTES: u64 = 64 * 1024 * 1024;

/// Extractor for camera RAW embedded previews via byte-range requests.
pub struct PreviewExtractor<B: RemoteObjectBackend> {
    backend: B,
}

impl<B: RemoteObjectBackend> PreviewExtractor<B> {
    /// Creates a new preview extractor backed by the given remote object backend.
    #[must_use]
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Identifies the container format from the initial bytes of an asset.
    #[must_use]
    pub fn detect_format(header: &[u8]) -> ContainerFormat {
        ContainerFormat::detect(header)
    }

    /// Extracts the embedded JPEG preview from a remote RAW image asset.
    ///
    /// # Arguments
    /// * `key` - Object path or filename in the backend.
    /// * `initial_chunk_bytes` - Byte size of initial header fetch (typically 128 KB).
    ///
    /// # Errors
    /// Returns an IO error if fetching fails, format is unsupported, preview exceeds safety cap, or tags are missing.
    pub async fn extract_preview(
        &self,
        key: &str,
        initial_chunk_bytes: u64,
    ) -> std::io::Result<Vec<u8>> {
        // Request 1: Fetch initial header
        let header = self.backend.read_range(key, 0, initial_chunk_bytes).await?;
        if header.len() < 8 {
            return Err(std::io::Error::other("File too small for RAW container"));
        }

        let format = Self::detect_format(&header);
        match format {
            ContainerFormat::TiffLittleEndian => {
                self.extract_tiff_preview_le(key, &header, initial_chunk_bytes)
                    .await
            }
            ContainerFormat::TiffBigEndian => Err(std::io::Error::other(
                "Big-endian TIFF preview extraction not yet implemented",
            )),
            ContainerFormat::IsoBmff => Err(std::io::Error::other(
                "ISOBMFF CR3 preview extraction not yet implemented",
            )),
            ContainerFormat::Unknown => {
                if header.starts_with(&[0xFF, 0xD8]) {
                    // For standard JPEG files, read up to bounded size
                    self.backend
                        .read_range(key, 0, MAX_PREVIEW_SIZE_BYTES)
                        .await
                } else {
                    Err(std::io::Error::other("Unrecognized RAW container format"))
                }
            }
        }
    }

    async fn extract_tiff_preview_le(
        &self,
        key: &str,
        header: &[u8],
        _initial_chunk: u64,
    ) -> std::io::Result<Vec<u8>> {
        let ifd_offset = u32::from_le_bytes(
            header
                .get(4..8)
                .and_then(|s| s.try_into().ok())
                .ok_or_else(|| std::io::Error::other("Invalid IFD offset slice"))?,
        ) as usize;

        let (mut preview_offset, mut preview_length, subifd_offset) =
            Self::parse_ifd_le(header, ifd_offset)?;

        // If not found in root IFD, follow SubIFD pointer (Tag 0x014A)
        if (preview_offset.is_none() || preview_length.is_none()) && subifd_offset.is_some() {
            if let Some(sub_off) = subifd_offset {
                let (sub_prev_off, sub_prev_len, _) = Self::parse_ifd_le(header, sub_off as usize)?;
                if sub_prev_off.is_some() && sub_prev_len.is_some() {
                    preview_offset = sub_prev_off;
                    preview_length = sub_prev_len;
                }
            }
        }

        if let (Some(offset), Some(length)) = (preview_offset, preview_length) {
            if length > MAX_PREVIEW_SIZE_BYTES {
                return Err(std::io::Error::other(format!(
                    "Embedded preview size ({length} bytes) exceeds safety cap of {MAX_PREVIEW_SIZE_BYTES} bytes"
                )));
            }

            // Request 2: Fetch exact preview byte range
            let preview_bytes = self.backend.read_range(key, offset, length).await?;
            if preview_bytes.len() < 4 || !preview_bytes.starts_with(&[0xFF, 0xD8]) {
                return Err(std::io::Error::other(
                    "Extracted preview does not begin with valid JPEG SOI magic bytes",
                ));
            }
            Ok(preview_bytes)
        } else {
            Err(std::io::Error::other(
                "Embedded preview tags (0x0201/0x0202) not found in root IFD or SubIFD",
            ))
        }
    }

    /// Parses an IFD block within the buffer for preview tags (0x0201, 0x0202) and SubIFD pointer (0x014A).
    fn parse_ifd_le(
        buf: &[u8],
        ifd_offset: usize,
    ) -> std::io::Result<(Option<u64>, Option<u64>, Option<u32>)> {
        let entry_count_end = ifd_offset
            .checked_add(2)
            .ok_or_else(|| std::io::Error::other("IFD offset calculation overflow"))?;

        if entry_count_end > buf.len() {
            return Err(std::io::Error::other(
                "IFD offset outside initial header chunk",
            ));
        }

        let num_entries = u16::from_le_bytes(
            buf[ifd_offset..entry_count_end]
                .try_into()
                .map_err(|_| std::io::Error::other("Invalid IFD entry count slice"))?,
        ) as usize;

        let mut preview_offset: Option<u64> = None;
        let mut preview_length: Option<u64> = None;
        let mut subifd_offset: Option<u32> = None;

        let mut cursor = entry_count_end;
        for _ in 0..num_entries {
            let entry_end = cursor
                .checked_add(12)
                .ok_or_else(|| std::io::Error::other("Cursor overflow in IFD"))?;
            if entry_end > buf.len() {
                break;
            }

            let tag = u16::from_le_bytes(
                buf[cursor..cursor + 2]
                    .try_into()
                    .map_err(|_| std::io::Error::other("Invalid tag"))?,
            );
            let val = u32::from_le_bytes(
                buf[cursor + 8..cursor + 12]
                    .try_into()
                    .map_err(|_| std::io::Error::other("Invalid tag value"))?,
            );

            match tag {
                0x0201 => preview_offset = Some(u64::from(val)),
                0x0202 => preview_length = Some(u64::from(val)),
                0x014A => subifd_offset = Some(val),
                _ => {}
            }

            cursor = entry_end;
        }

        Ok((preview_offset, preview_length, subifd_offset))
    }
}
