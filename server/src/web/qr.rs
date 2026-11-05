//! QR codes as SVG markup inside the page, drawn on the server, so the page
//! needs no script and no image request to show one.

use std::fmt::Write;

use fast_qr::{ECL, QRBuilder};

/// The light modules a scanner needs around the code, by the standard.
const QUIET_ZONE: usize = 4;

/// `text` as an SVG QR code, or `None` when it is longer than a QR code
/// holds. Each row's dark modules are one rectangle per run, which keeps
/// the markup small. The page's stylesheet colors it, dark on light in
/// either scheme, since not every scanner reads a code inverted.
pub(crate) fn svg(text: &str) -> Option<String> {
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
        r#"<svg class="qr" viewBox="0 0 {side} {side}" role="img" aria-label="A QR code of this address" shape-rendering="crispEdges"><rect width="{side}" height="{side}"/><path d="{path}"/></svg>"#
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn text_longer_than_a_code_holds_has_no_code() {
        assert_eq!(super::svg(&"a".repeat(3000)), None);
    }
}
