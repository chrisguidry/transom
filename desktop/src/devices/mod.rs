//! This person's devices: every browser, phone, and computer the server
//! reaches them on, and the code a phone scans to sign in.

pub mod screen;

use std::fmt::Write;

use fast_qr::{ECL, QRBuilder};

/// The light modules a scanner needs around the code, by the standard.
const QUIET_ZONE: usize = 4;

/// How many pixels the image is across, twice what the page shows, so
/// the code stays sharp on a dense screen.
const PIXELS: usize = 384;

/// `text` as an SVG QR code, dark on light whatever the scheme, since
/// not every scanner reads a code drawn inverted, or `None` when the
/// text is longer than a QR code holds. Each row's dark modules are one
/// rectangle per run, as the web UI draws its code.
pub fn qr(text: &str) -> Option<String> {
    let code = QRBuilder::new(text).ecl(ECL::M).build().ok()?;
    let mut path = String::new();
    for y in 0..code.size {
        let row = &code[y];
        let mut x = 0;
        while x < code.size {
            let run = row[x..].iter().take_while(|module| module.value()).count();
            if run > 0 {
                let (left, top) = (x + QUIET_ZONE, y + QUIET_ZONE);
                // Writing to a String does not fail.
                let _ = write!(path, "M{left} {top}h{run}v1h-{run}z");
            }
            x += run.max(1);
        }
    }
    let side = code.size + 2 * QUIET_ZONE;
    Some(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{PIXELS}" height="{PIXELS}" viewBox="0 0 {side} {side}" shape-rendering="crispEdges"><rect width="{side}" height="{side}" fill="#ffffff"/><path d="{path}" fill="#23262b"/></svg>"##
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_has_a_code_and_text_too_long_has_none() {
        let code = qr("https://transom.example.com/").unwrap();
        assert!(code.starts_with("<svg"));
        assert!(code.contains("fill=\"#23262b\""));
        assert_eq!(qr(&"a".repeat(3000)), None);
    }
}
