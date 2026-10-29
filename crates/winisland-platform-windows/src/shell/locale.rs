use windows::Win32::Globalization::{GetLocaleInfoEx, GetUserDefaultLocaleName, LOCALE_SSHORTTIME};
use windows::core::PCWSTR;
use winisland_platform::HourFormat;

pub(super) fn system_locale() -> String {
    let mut buffer = [0u16; 128];
    // SAFETY: The buffer is writable for all 128 UTF-16 code units.
    let len = unsafe { GetUserDefaultLocaleName(&mut buffer) };
    if len > 0 {
        String::from_utf16_lossy(&buffer[..len as usize - 1])
    } else {
        String::new()
    }
}

pub(super) fn hour_format() -> HourFormat {
    let mut buffer = [0u16; 80];
    // SAFETY: A null locale name selects the user default locale, including the user's
    // overrides from Settings; the buffer is writable for all 80 UTF-16 code units.
    let len = unsafe { GetLocaleInfoEx(PCWSTR::null(), LOCALE_SSHORTTIME, Some(&mut buffer)) };
    let Some(len) = usize::try_from(len).ok().filter(|len| *len > 0) else {
        return HourFormat::default();
    };
    parse_hour_format(&String::from_utf16_lossy(&buffer[..len - 1]))
}

/// Reads the hour token of a Win32 time picture such as `HH:mm` or `tt h:mm`.
fn parse_hour_format(pattern: &str) -> HourFormat {
    let mut quoted = false;
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\'' => quoted = !quoted,
            'h' | 'H' if !quoted => {
                return HourFormat {
                    twelve_hour: ch == 'h',
                    zero_padded: chars.peek() == Some(&ch),
                };
            }
            _ => {}
        }
    }
    HourFormat::default()
}
