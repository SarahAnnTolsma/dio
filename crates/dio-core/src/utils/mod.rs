//! Shared utility functions used by multiple transformers.

pub mod eval;

const STANDARD_BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Decode a base64-encoded string using the standard alphabet.
///
/// Treats `=` as padding (standard base64 behavior).
pub fn base64_decode(input: &str) -> Option<String> {
    decode_base64_impl(input, STANDARD_BASE64_ALPHABET, true)
}

/// Decode a base64-encoded string using a custom 64-character alphabet.
///
/// The alphabet must be exactly 64 bytes. Unlike standard base64, `=` is NOT
/// treated as padding — it may be a valid character in the custom alphabet.
/// Characters not in the alphabet are skipped.
pub fn base64_decode_with_alphabet(input: &str, alphabet: &[u8]) -> Option<String> {
    decode_base64_impl(input, alphabet, false)
}

fn decode_base64_impl(input: &str, alphabet: &[u8], stop_at_equals: bool) -> Option<String> {
    // Accept 64-char alphabets (standard) or 65-char alphabets where the
    // 65th character is the padding sentinel (common in obfuscators).
    if alphabet.len() != 64 && alphabet.len() != 65 {
        return None;
    }

    // Build lookup table: byte -> 6-bit value (0-63), or 255 for unknown.
    // For 65-char alphabets, the 65th character (index 64) is the padding
    // sentinel — we map it to 64 so the bit-shift loop skips padding bytes.
    let mut lookup = [255u8; 256];
    for (index, &byte) in alphabet.iter().enumerate() {
        lookup[byte as usize] = index as u8;
    }

    let mut output = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits_collected: u32 = 0;

    for byte in input.bytes() {
        if stop_at_equals && byte == b'=' {
            break;
        }

        let value = lookup[byte as usize];
        if value == 255 || value >= 64 {
            // Unknown character or padding sentinel — skip.
            continue;
        }

        buffer = (buffer << 6) | u32::from(value);
        bits_collected += 6;

        if bits_collected >= 8 {
            bits_collected -= 8;
            output.push((buffer >> bits_collected) as u8);
            buffer &= (1 << bits_collected) - 1;
        }
    }

    // Convert bytes to string using Latin-1 (byte-as-char), matching JS atob behavior.
    Some(output.iter().map(|&b| b as char).collect())
}

/// Encode bytes to a base64 string using the standard alphabet.
pub fn base64_encode(input: &[u8]) -> String {
    let mut output = String::new();

    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = if chunk.len() > 1 { u32::from(chunk[1]) } else { 0 };
        let b2 = if chunk.len() > 2 { u32::from(chunk[2]) } else { 0 };

        let triple = (b0 << 16) | (b1 << 8) | b2;

        output.push(STANDARD_BASE64_ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        output.push(STANDARD_BASE64_ALPHABET[((triple >> 12) & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            output.push(STANDARD_BASE64_ALPHABET[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            output.push('=');
        }

        if chunk.len() > 2 {
            output.push(STANDARD_BASE64_ALPHABET[(triple & 0x3F) as usize] as char);
        } else {
            output.push('=');
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_base64_decode() {
        assert_eq!(base64_decode("TnVtYmVy").unwrap(), "Number");
        assert_eq!(base64_decode("ZnVuY3Rpb24").unwrap(), "function");
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), "hello");
    }

    #[test]
    fn custom_alphabet_decode() {
        let alphabet = b"zTDpQgXBRVofJM=xaA2u6s3iKm5tlZr1LHdCwn0WjUINh4bO/vk8eEYF7qGc+y9SP";
        let result = base64_decode_with_alphabet("u3ge5zPP", alphabet).unwrap();
        // This should decode to a valid string using the custom alphabet
        assert!(!result.is_empty());
    }

    #[test]
    fn standard_base64_roundtrip() {
        let original = "Hello, World!";
        let encoded = base64_encode(original.as_bytes());
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }
}
