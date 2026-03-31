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

/// Decode base64 to raw bytes (no Latin-1 string conversion).
fn base64_decode_to_bytes(input: &str, alphabet: &[u8]) -> Option<Vec<u8>> {
    if alphabet.len() != 64 && alphabet.len() != 65 {
        return None;
    }

    let mut lookup = [255u8; 256];
    for (index, &byte) in alphabet.iter().enumerate() {
        lookup[byte as usize] = index as u8;
    }

    let mut output = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits_collected: u32 = 0;

    for byte in input.bytes() {
        let value = lookup[byte as usize];
        if value == 255 || value >= 64 {
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

    Some(output)
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

/// Base64 alphabet used by Obfuscator.io's RC4 decoder (lowercase first).
const OBFUSCATOR_IO_BASE64_ALPHABET: &[u8; 65] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+/=";

/// Decode a base64-encoded string and then RC4-decrypt it with the given key.
///
/// Matches the Obfuscator.io "high" obfuscation pattern where each string
/// array entry is base64-encoded, then RC4-encrypted using a per-call key.
/// Uses the lowercase-first base64 alphabet specific to Obfuscator.io.
///
/// The JS decoder chain is:
/// 1. Base64 decode to raw bytes
/// 2. Percent-encode each byte and `decodeURIComponent` to get a UTF-8 string
/// 3. RC4 decrypt treating character codes as the data stream
pub fn base64_rc4_decode(encoded: &str, key: &str) -> Option<String> {
    // Step 1: Base64 decode to raw bytes.
    let bytes = base64_decode_to_bytes(encoded, OBFUSCATOR_IO_BASE64_ALPHABET)?;

    // Step 2: Interpret the raw bytes as UTF-8 (matching JS decodeURIComponent
    // on percent-encoded bytes). This converts multi-byte UTF-8 sequences into
    // single characters.
    let utf8_string = String::from_utf8(bytes).ok()?;

    // Step 3: RC4 decrypt on the character codes.
    // The JS decoder operates on charCodeAt values, so we collect char codes
    // as bytes for RC4 (char codes > 255 would be unusual in this context).
    let char_codes: Vec<u8> = utf8_string
        .chars()
        .map(|character| character as u32 as u8)
        .collect();
    let key_bytes: Vec<u8> = key
        .chars()
        .map(|character| character as u32 as u8)
        .collect();
    let decrypted = rc4_decrypt(&char_codes, &key_bytes);

    // Step 4: Interpret as UTF-8 (fall back to Latin-1).
    match String::from_utf8(decrypted.clone()) {
        Ok(string) => Some(string),
        Err(_) => Some(decrypted.iter().map(|&b| b as char).collect()),
    }
}

/// RC4 stream cipher decryption (symmetric — encryption and decryption are the same).
fn rc4_decrypt(data: &[u8], key: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return data.to_vec();
    }

    // Key-Scheduling Algorithm (KSA)
    let mut state = [0u8; 256];
    for i in 0..256 {
        state[i] = i as u8;
    }
    let mut j: u8 = 0;
    for i in 0..256 {
        j = j
            .wrapping_add(state[i])
            .wrapping_add(key[i % key.len()]);
        state.swap(i, j as usize);
    }

    // Pseudo-Random Generation Algorithm (PRGA)
    let mut output = Vec::with_capacity(data.len());
    let mut i: u8 = 0;
    let mut j: u8 = 0;
    for &byte in data {
        i = i.wrapping_add(1);
        j = j.wrapping_add(state[i as usize]);
        state.swap(i as usize, j as usize);
        let keystream_byte = state[(state[i as usize].wrapping_add(state[j as usize])) as usize];
        output.push(byte ^ keystream_byte);
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
    fn base64_rc4_decode_raw_bytes() {
        // "WOXYW5Daxq" decoded with lowercase-first alphabet gives bytes [194,140,114,195,151,64,93]
        let bytes =
            super::base64_decode_to_bytes("WOXYW5Daxq", super::OBFUSCATOR_IO_BASE64_ALPHABET)
                .unwrap();
        assert_eq!(bytes, vec![194, 140, 114, 195, 151, 64, 93]);
    }

    #[test]
    fn rc4_decrypt_basic() {
        // Verify RC4 is symmetric: encrypt then decrypt should give original
        let data = b"Hello, World!";
        let key = b"test_key";
        let encrypted = super::rc4_decrypt(data, key);
        let decrypted = super::rc4_decrypt(&encrypted, key);
        assert_eq!(decrypted, data);
    }

    #[test]
    fn standard_base64_roundtrip() {
        let original = "Hello, World!";
        let encoded = base64_encode(original.as_bytes());
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }
}
