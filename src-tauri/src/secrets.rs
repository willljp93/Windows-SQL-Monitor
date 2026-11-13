use base64::{engine::general_purpose::STANDARD, Engine as _};

#[cfg(windows)]
pub fn protect_secret(plain: &str) -> String {
    if plain.is_empty() {
        return String::new();
    }
    if plain.starts_with("dpapi:") {
        return plain.to_string();
    }
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};

    let bytes = plain.as_bytes();
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    unsafe {
        let ok = CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut output,
        );
        if ok != 0 && !output.pbData.is_null() {
            let slice = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
            let encoded = STANDARD.encode(slice);
            LocalFree(output.pbData as _);
            return format!("dpapi:{}", encoded);
        }
    }
    plain.to_string()
}

#[cfg(windows)]
pub fn unprotect_secret(stored: &str) -> String {
    if let Some(b64) = stored.strip_prefix("dpapi:") {
        if let Ok(raw) = STANDARD.decode(b64) {
            use windows_sys::Win32::Foundation::LocalFree;
            use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};

            let input = CRYPT_INTEGER_BLOB {
                cbData: raw.len() as u32,
                pbData: raw.as_ptr() as *mut u8,
            };
            let mut output = CRYPT_INTEGER_BLOB {
                cbData: 0,
                pbData: std::ptr::null_mut(),
            };

            unsafe {
                let ok = CryptUnprotectData(
                    &input,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    &mut output,
                );
                if ok != 0 && !output.pbData.is_null() {
                    let slice = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
                    let plain = String::from_utf8_lossy(slice).to_string();
                    LocalFree(output.pbData as _);
                    return plain;
                }
            }
        }
    }
    stored.to_string()
}

#[cfg(not(windows))]
pub fn protect_secret(plain: &str) -> String {
    plain.to_string()
}

#[cfg(not(windows))]
pub fn unprotect_secret(stored: &str) -> String {
    stored.to_string()
}
