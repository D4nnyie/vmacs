use std::io::{Error, ErrorKind};

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Encoding {
    #[default]
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Latin1,
    Windows1252,
}

/// Windows-1252 bytes 0x80..=0x9F as Unicode code points.
const CP1252_HIGH: [u32; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
    0x2039, 0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022,
    0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
];

fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message.to_string())
}

impl Encoding {
    pub const NAMES: &'static str = "utf-8, utf-8-bom, utf-16le, utf-16be, latin1, cp1252";

    pub const fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf8Bom => "UTF-8 BOM",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
            Self::Latin1 => "Latin-1",
            Self::Windows1252 => "CP1252",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        let key: String = name
            .trim()
            .to_lowercase()
            .chars()
            .filter(|c| *c != '-' && *c != '_')
            .collect();
        match key.as_str() {
            "utf8" => Some(Self::Utf8),
            "utf8bom" | "utf8withbom" => Some(Self::Utf8Bom),
            "utf16" | "utf16le" | "ucs2le" => Some(Self::Utf16Le),
            "utf16be" => Some(Self::Utf16Be),
            "latin1" | "iso88591" | "l1" => Some(Self::Latin1),
            "cp1252" | "windows1252" | "win1252" => Some(Self::Windows1252),
            _ => None,
        }
    }

    /// Guess the encoding: BOM first, then valid UTF-8, then CP1252.
    pub fn detect(bytes: &[u8]) -> Result<(String, Self), Error> {
        let by_bom = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            Some(Self::Utf8Bom)
        } else if bytes.starts_with(&[0xFF, 0xFE]) {
            Some(Self::Utf16Le)
        } else if bytes.starts_with(&[0xFE, 0xFF]) {
            Some(Self::Utf16Be)
        } else {
            None
        };
        if let Some(encoding) = by_bom {
            if let Ok(text) = encoding.decode_as(bytes) {
                return Ok((text, encoding));
            }
        }
        if bytes.contains(&0) {
            return Err(invalid("binary file (contains NUL bytes)"));
        }
        match std::str::from_utf8(bytes) {
            Ok(text) => Ok((text.to_string(), Self::Utf8)),
            Err(_) => Self::Windows1252
                .decode_as(bytes)
                .map(|text| (text, Self::Windows1252)),
        }
    }

    pub fn decode_as(self, bytes: &[u8]) -> Result<String, Error> {
        let bad_utf8 = || invalid("not valid UTF-8 (try :reopen latin1 or :reopen cp1252)");
        match self {
            Self::Utf8 => std::str::from_utf8(bytes)
                .map(str::to_string)
                .map_err(|_| bad_utf8()),
            Self::Utf8Bom => {
                let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
                std::str::from_utf8(body)
                    .map(str::to_string)
                    .map_err(|_| bad_utf8())
            }
            Self::Utf16Le | Self::Utf16Be => {
                let le = self == Self::Utf16Le;
                let bom: &[u8] = if le { &[0xFF, 0xFE] } else { &[0xFE, 0xFF] };
                let body = bytes.strip_prefix(bom).unwrap_or(bytes);
                if body.len() & 1 == 1 {
                    return Err(invalid("UTF-16 data has an odd number of bytes"));
                }
                let units: Vec<u16> = body
                    .chunks_exact(2)
                    .map(|pair| {
                        let raw = [pair[0], pair[1]];
                        if le { u16::from_le_bytes(raw) } else { u16::from_be_bytes(raw) }
                    })
                    .collect();
                String::from_utf16(&units).map_err(|_| invalid("invalid UTF-16 data"))
            }
            Self::Latin1 => Ok(bytes.iter().map(|b| char::from(*b)).collect()),
            Self::Windows1252 => Ok(bytes
                .iter()
                .map(|&b| {
                    if (0x80..0xA0).contains(&b) {
                        char::from_u32(CP1252_HIGH[usize::from(b - 0x80)]).unwrap_or('\u{fffd}')
                    } else {
                        char::from(b)
                    }
                })
                .collect()),
        }
    }

    fn unmappable(self, c: char) -> String {
        format!("'{c}' cannot be saved as {} (use :encoding utf-8)", self.label())
    }

    pub fn encode(self, text: &str) -> Result<Vec<u8>, String> {
        match self {
            Self::Utf8 => Ok(text.as_bytes().to_vec()),
            Self::Utf8Bom => {
                let mut out = vec![0xEF, 0xBB, 0xBF];
                out.extend_from_slice(text.as_bytes());
                Ok(out)
            }
            Self::Utf16Le | Self::Utf16Be => {
                let le = self == Self::Utf16Le;
                let mut out = if le { vec![0xFF, 0xFE] } else { vec![0xFE, 0xFF] };
                for unit in text.encode_utf16() {
                    let bytes = if le { unit.to_le_bytes() } else { unit.to_be_bytes() };
                    out.extend_from_slice(&bytes);
                }
                Ok(out)
            }
            Self::Latin1 => text
                .chars()
                .map(|c| u8::try_from(u32::from(c)).map_err(|_| self.unmappable(c)))
                .collect(),
            Self::Windows1252 => text
                .chars()
                .map(|c| {
                    let n = u32::from(c);
                    if n < 0x80 || (0xA0..=0xFF).contains(&n) {
                        u8::try_from(n).map_err(|_| self.unmappable(c))
                    } else {
                        CP1252_HIGH
                            .iter()
                            .position(|&u| u == n)
                            .and_then(|i| u8::try_from(i + 0x80).ok())
                            .ok_or_else(|| self.unmappable(c))
                    }
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_order() {
        assert_eq!(Encoding::detect(b"abc").unwrap().1, Encoding::Utf8);
        assert_eq!(Encoding::detect(b"\xef\xbb\xbfabc").unwrap().1, Encoding::Utf8Bom);
        assert_eq!(Encoding::detect(b"\xff\xfea\x00").unwrap(), ("a".to_string(), Encoding::Utf16Le));
        assert_eq!(Encoding::detect(b"\xfe\xff\x00a").unwrap(), ("a".to_string(), Encoding::Utf16Be));
        assert_eq!(Encoding::detect(b"caf\xe9").unwrap(), ("café".to_string(), Encoding::Windows1252));
        assert!(Encoding::detect(b"a\x00b").is_err());
    }

    #[test]
    fn encode_roundtrips() {
        for encoding in [Encoding::Utf8, Encoding::Utf8Bom, Encoding::Utf16Le, Encoding::Utf16Be] {
            let bytes = encoding.encode("héllo ✓").unwrap();
            assert_eq!(encoding.decode_as(&bytes).unwrap(), "héllo ✓", "{encoding:?}");
        }
        let bytes = Encoding::Windows1252.encode("€é").unwrap();
        assert_eq!(bytes, vec![0x80, 0xE9]);
        assert!(Encoding::Latin1.encode("€").is_err());
    }

    #[test]
    fn names_parse() {
        assert_eq!(Encoding::parse("UTF-8"), Some(Encoding::Utf8));
        assert_eq!(Encoding::parse("utf_16le"), Some(Encoding::Utf16Le));
        assert_eq!(Encoding::parse("ISO-8859-1"), Some(Encoding::Latin1));
        assert_eq!(Encoding::parse("sjis"), None);
    }
}