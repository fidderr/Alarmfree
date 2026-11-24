//! Base64 as everyone writes it (standard letters, `=` padding), for the photo on the ring screen
//! and the sounds in a backup file.

const LETTERS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(bytes: &[u8]) -> String {
    let letter = |index: u32| char::from(LETTERS[(index & 63) as usize]);
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let [a, b, c] = [
            u32::from(chunk[0]),
            u32::from(*chunk.get(1).unwrap_or(&0)),
            u32::from(*chunk.get(2).unwrap_or(&0)),
        ];
        let all = (a << 16) | (b << 8) | c;
        text.push(letter(all >> 18));
        text.push(letter(all >> 12));
        text.push(if chunk.len() > 1 {
            letter(all >> 6)
        } else {
            '='
        });
        text.push(if chunk.len() > 2 { letter(all) } else { '=' });
    }
    text
}

/// The bytes of `text`, or `None` when it is not base64.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let digits = text.trim_end_matches('=').as_bytes();
    if text.len() % 4 != 0 || text.len() - digits.len() > 2 {
        return None;
    }
    let mut bytes = Vec::with_capacity(digits.len() / 4 * 3 + 2);
    for chunk in digits.chunks(4) {
        let mut all = 0u32;
        for (place, digit) in chunk.iter().enumerate() {
            let value = LETTERS.iter().position(|letter| letter == digit)? as u32;
            all |= value << (18 - 6 * place);
        }
        let used = chunk.len() * 6 / 8;
        bytes.extend_from_slice(&all.to_be_bytes()[1..=used]);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_as_everyone_writes_it() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
        assert_eq!(encode(&[0xfb, 0xff, 0xfe]), "+//+");
    }

    #[test]
    fn what_was_written_reads_back() {
        for bytes in [
            b"".as_slice(),
            b"f",
            b"fo",
            b"foo",
            b"foob",
            &[0xfb, 0xff, 0xfe],
            &[0, 1, 2, 3, 4, 5, 6, 250, 251, 252, 253, 254, 255],
        ] {
            assert_eq!(decode(&encode(bytes)).as_deref(), Some(bytes));
        }
    }

    #[test]
    fn text_that_is_not_base64_is_refused() {
        for bad in ["Zg=", "Zg", "Z===", "Zm9v!", "Zm 9v", "Zg==Zg=="] {
            assert_eq!(decode(bad), None, "{bad}");
        }
    }
}
