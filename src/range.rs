#[derive(Debug, PartialEq, Eq)]
pub enum ByteRange {
    Full,
    Partial { start: u64, end: u64 },
    Invalid,
}

pub fn parse(header: Option<&str>, size: u64) -> ByteRange {
    let Some(header) = header else {
        return ByteRange::Full;
    };
    let Some(value) = header.trim().strip_prefix("bytes=") else {
        return ByteRange::Invalid;
    };
    if value.contains(',') || size == 0 {
        return ByteRange::Invalid;
    }
    let Some((start, end)) = value.split_once('-') else {
        return ByteRange::Invalid;
    };

    if start.is_empty() {
        let Ok(suffix) = end.parse::<u64>() else {
            return ByteRange::Invalid;
        };
        if suffix == 0 {
            return ByteRange::Invalid;
        }
        return ByteRange::Partial {
            start: size.saturating_sub(suffix),
            end: size - 1,
        };
    }

    let Ok(start) = start.parse::<u64>() else {
        return ByteRange::Invalid;
    };
    let end = if end.is_empty() {
        size - 1
    } else {
        let Ok(end) = end.parse::<u64>() else {
            return ByteRange::Invalid;
        };
        end.min(size - 1)
    };

    if start >= size || end < start {
        ByteRange::Invalid
    } else {
        ByteRange::Partial { start, end }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_closed_open_and_suffix_ranges() {
        assert_eq!(parse(None, 1_000), ByteRange::Full);
        assert_eq!(
            parse(Some("bytes=100-199"), 1_000),
            ByteRange::Partial {
                start: 100,
                end: 199
            }
        );
        assert_eq!(
            parse(Some("bytes=900-2000"), 1_000),
            ByteRange::Partial {
                start: 900,
                end: 999
            }
        );
        assert_eq!(
            parse(Some("bytes=250-"), 1_000),
            ByteRange::Partial {
                start: 250,
                end: 999
            }
        );
        assert_eq!(
            parse(Some("bytes=-100"), 1_000),
            ByteRange::Partial {
                start: 900,
                end: 999
            }
        );
    }

    #[test]
    fn rejects_malformed_and_unsatisfiable_ranges() {
        assert_eq!(parse(Some("items=0-10"), 1_000), ByteRange::Invalid);
        assert_eq!(parse(Some("bytes=0-1,4-5"), 1_000), ByteRange::Invalid);
        assert_eq!(parse(Some("bytes=1000-"), 1_000), ByteRange::Invalid);
        assert_eq!(parse(Some("bytes=20-10"), 1_000), ByteRange::Invalid);
    }
}
