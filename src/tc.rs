//! The timecode fields "Film starts at / ends at": what a key, a click or a paste does to the field's text and to
//! the part (hours … frames) that is chosen. Pure functions, the field in ui/app.slint sends every key here.

use crate::TcState;

/// A typed timecode: digits only, filled from the right like an Avid TC field ("1234" → "00:00:12:34"), leading
/// zeros give way to new digits; all zero = empty (no limit). More than 8 digits: `before` stays.
pub fn tc_typed(typed: &str, before: &str) -> String {
    let digits: String = typed.chars().filter(char::is_ascii_digit).collect();
    let d = digits.trim_start_matches('0');
    match d.len() {
        0 => String::new(),
        9.. => before.to_string(),
        _ => {
            let d = format!("{d:0>8}");
            format!("{}:{}:{}:{}", &d[0..2], &d[2..4], &d[4..6], &d[6..8])
        }
    }
}

/// A key in a timecode field: a digit is added at the right, Backspace / Delete takes the last digit off, anything
/// else changes nothing.
pub fn tc_key(text: &str, key: &str) -> String {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    match key {
        "\u{8}" | "\u{7f}" => tc_typed(&digits[..digits.len().saturating_sub(1)], text),
        k if k.len() == 1 && k.chars().all(|c| c.is_ascii_digit()) => tc_typed(&(digits + k), text),
        _ => text.to_string(),
    }
}

/// The four parts of a timecode field (hours, minutes, seconds, frames); empty = all zero.
pub fn tc_parts(text: &str) -> [u32; 4] {
    let mut p = [0; 4];
    for (k, v) in text.split(':').filter_map(|v| v.parse().ok()).take(4).enumerate() {
        p[k] = v;
    }
    p
}

pub fn tc_text(p: [u32; 4]) -> String {
    if p == [0; 4] { String::new() } else { format!("{:02}:{:02}:{:02}:{:02}", p[0], p[1], p[2], p[3]) }
}

/// A key in a timecode field, as in the timecode fields of editing apps. `sel`: -1 nothing, 0–3 one part chosen
/// (hours … frames), 4 everything chosen, 5 typing the whole timecode; `typed`: digits typed into the chosen part.
/// A part: two digits replace it (one: "0d") and the next part is chosen, ↑ / ↓ count it up / down, ← / → and ":"
/// choose the neighbour, Backspace sets it to 00. Everything (⌘A, double-click, Tab): a digit starts a new timecode
/// filled from the right like in Avid, Backspace empties the field. `last_frame`: the highest frame number.
pub fn tc_edit(st: TcState, key: &str, meta: bool, last_frame: u32) -> TcState {
    let mut p = tc_parts(&st.text);
    let max = [23, 59, 59, last_frame];
    let (mut sel, mut typed) = (st.sel, 0);
    let part = if (0..4).contains(&sel) { sel as usize } else { 3 };
    let mut text = None;
    match key {
        "a" | "A" if meta => sel = 4,
        _ if meta => return st,
        "\u{f700}" | "\u{f701}" => {
            let v = p[part] as i64 + if key == "\u{f700}" { 1 } else { -1 };
            p[part] = v.rem_euclid(max[part] as i64 + 1) as u32;
            sel = part as i32;
        }
        "\u{f702}" => sel = if (0..4).contains(&sel) { (sel - 1).max(0) } else { 0 },
        "\u{f703}" | ":" | "." => sel = if (0..4).contains(&sel) { (sel + 1).min(3) } else { 3 },
        "\u{8}" | "\u{7f}" => match sel {
            4 => {
                text = Some(String::new());
                sel = 5;
            }
            0..=3 => p[part] = 0,
            _ => text = Some(tc_key(&st.text, key)),
        },
        k if k.len() == 1 && k.as_bytes()[0].is_ascii_digit() => {
            let d = k.parse::<u32>().unwrap_or(0);
            match sel {
                0..=3 if st.typed == 0 => {
                    p[part] = d.min(max[part]);
                    typed = 1;
                }
                0..=3 => {
                    p[part] = ((p[part] % 10) * 10 + d).min(max[part]);
                    sel = (sel + 1).min(3);
                }
                4 => {
                    text = Some(tc_typed(k, ""));
                    sel = 5;
                }
                _ => {
                    text = Some(tc_key(&st.text, k));
                    sel = 5;
                }
            }
        }
        _ => return st,
    }
    TcState { text: text.unwrap_or_else(|| tc_text(p)).into(), sel, typed }
}

/// Text on the clipboard (macOS).
pub fn clipboard_text() -> Option<String> {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        let board: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        let kind = std::ffi::CString::new("public.utf8-plain-text").ok()?;
        let kind: *mut AnyObject = msg_send![class!(NSString), stringWithUTF8String: kind.as_ptr()];
        let s: *mut AnyObject = msg_send![board, stringForType: kind];
        if s.is_null() {
            return None;
        }
        let c: *const std::ffi::c_char = msg_send![s, UTF8String];
        return (!c.is_null()).then(|| std::ffi::CStr::from_ptr(c).to_string_lossy().into_owned());
    }
    #[allow(unreachable_code)]
    None
}

/// ⌘V in a timecode field: "01:02:03:04", "01020304" or "1:2:3:4" from the clipboard; anything else changes nothing.
pub fn tc_pasted(st: TcState, clip: &str, last_frame: u32) -> TcState {
    let clip = clip.trim();
    let parts: Vec<&str> = clip.split([':', ';', '.']).collect();
    let p: Option<Vec<u32>> = if parts.len() == 4 {
        parts.iter().map(|v| v.parse().ok()).collect()
    } else if (1..=8).contains(&clip.len()) && clip.bytes().all(|b| b.is_ascii_digit()) {
        Some(tc_parts(&tc_typed(clip, "")).to_vec())
    } else {
        None
    };
    match p {
        Some(p) if p[0] <= 23 && p[1] <= 59 && p[2] <= 59 && p[3] <= last_frame => TcState { text: tc_text([p[0], p[1], p[2], p[3]]).into(), sel: 4, typed: 0 },
        _ => st,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timecode_fields_fill_from_the_right() {
        use tc_typed;
        assert_eq!(tc_typed("1", ""), "00:00:00:01");
        assert_eq!(tc_typed("00:00:00:012", "00:00:00:01"), "00:00:00:12");
        assert_eq!(tc_typed("01:00:00:00", ""), "01:00:00:00");
        assert_eq!(tc_typed("01:00:00:000", "01:00:00:00"), "10:00:00:00"); // a leading zero gives way
        assert_eq!(tc_typed("10:00:00:000", "10:00:00:00"), "10:00:00:00"); // a 9th digit is refused
        assert_eq!(tc_typed("00:00:00:1", "00:00:00:12"), "00:00:00:01"); // backspace shifts right
        assert_eq!(tc_typed("00:00:00:0", "00:00:00:01"), "");
        assert_eq!(tc_typed("1a:b2", ""), "00:00:00:12");
        let typed = ["1", "a", "0", ":", "0", "1", "3", "0", "0", "0", "5"].iter().fold(String::new(), |t, k| tc_key(&t, k));
        assert_eq!(typed, "10:01:30:00"); // the 9th digit (5) is refused
        assert_eq!(tc_key("10:01:30:00", "\u{8}"), "01:00:13:00");
        assert_eq!(tc_key("00:00:00:07", "\u{7f}"), "");
    }

    #[test]
    fn timecode_parts_are_chosen_and_typed_like_in_editing_apps() {
        use crate::TcState;
        let st = |t: &str, sel: i32, typed: i32| TcState { text: t.into(), sel, typed };
        let keys = |mut s: TcState, ks: &[&str]| {
            for k in ks {
                s = tc_edit(s, k, false, 24);
            }
            (s.text.to_string(), s.sel, s.typed)
        };
        // hours chosen: two digits replace them, the minutes come next
        assert_eq!(keys(st("10:20:30:12", 0, 0), &["0", "1"]), ("01:20:30:12".into(), 1, 0));
        assert_eq!(keys(st("10:20:30:12", 0, 0), &["7"]), ("07:20:30:12".into(), 0, 1));
        // too large: the highest the part allows
        assert_eq!(keys(st("", 1, 0), &["9", "9"]), ("00:59:00:00".into(), 2, 0));
        assert_eq!(keys(st("", 3, 0), &["3", "0"]), ("00:00:00:24".into(), 3, 0));
        // up / down count the chosen part round; left / right choose
        assert_eq!(keys(st("00:59:00:00", 1, 0), &["\u{f700}"]), ("".into(), 1, 0));
        assert_eq!(keys(st("", 3, 0), &["\u{f701}"]), ("00:00:00:24".into(), 3, 0));
        assert_eq!(keys(st("01:00:00:00", 2, 0), &["\u{f702}", "\u{f702}", "\u{f702}"]).1, 0);
        // Backspace: the part to 00; everything chosen: empty, then typed from the right
        assert_eq!(keys(st("01:02:03:04", 1, 0), &["\u{8}"]).0, "01:00:03:04");
        assert_eq!(keys(st("01:02:03:04", 4, 0), &["\u{8}"]), ("".into(), 5, 0));
        assert_eq!(keys(st("01:02:03:04", 4, 0), &["1", "2", "3"]), ("00:00:01:23".into(), 5, 0));
        assert_eq!(tc_edit(st("01:02:03:04", 1, 0), "a", true, 24).sel, 4);
        assert_eq!(tc_edit(st("01:02:03:04", 1, 0), "x", false, 24).text, "01:02:03:04");
        // paste
        assert_eq!(tc_pasted(st("", 1, 0), " 01:00:10:00\n", 24).text, "01:00:10:00");
        assert_eq!(tc_pasted(st("", 1, 0), "1001000", 24).text, "01:00:10:00");
        assert_eq!(tc_pasted(st("09:00:00:00", 1, 0), "hello", 24).text, "09:00:00:00");
        assert_eq!(tc_pasted(st("", 1, 0), "01:00:10:30", 24).text, ""); // frame 30 at 25 fps
    }
}
