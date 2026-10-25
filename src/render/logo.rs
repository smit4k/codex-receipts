// One pixel master keeps the terminal and SVG marks consistent.
const PIXELS: [&str; 12] = [
    "###                     ",
    " ###                    ",
    "  ###                   ",
    "   ###                  ",
    "    ###                 ",
    "     ###                ",
    "     ###                ",
    "    ###                 ",
    "   ###                  ",
    "  ###         ##########",
    " ###          ##########",
    "###           ##########",
];

pub fn text(width: usize, blocks: bool) -> String {
    let pixels: Vec<Vec<bool>> = PIXELS
        .chunks(2)
        .map(|rows| {
            (0..24)
                .step_by(2)
                .map(|x| {
                    rows.iter()
                        .any(|row| row.as_bytes()[x..x + 2].contains(&b'#'))
                })
                .collect()
        })
        .collect();
    let padding = " ".repeat((width - pixels[0].len()) / 2);
    let mut output = String::new();
    for pair in pixels.chunks(2) {
        let row: String = pair[0]
            .iter()
            .zip(&pair[1])
            .map(|(&top, &bottom)| {
                if blocks {
                    match (top, bottom) {
                        (true, true) => '█',
                        (true, false) => '▀',
                        (false, true) => '▄',
                        (false, false) => ' ',
                    }
                } else if top || bottom {
                    '#'
                } else {
                    ' '
                }
            })
            .collect();
        output.push_str(&format!("{padding}{}\n", row.trim_end()));
    }
    output
}

pub fn svg() -> String {
    let mut svg = String::from(
        r#"<svg class="mark" viewBox="0 0 24 12" role="img" aria-label="Codex command prompt" xmlns="http://www.w3.org/2000/svg"><title>Codex command prompt</title>"#,
    );
    for (y, row) in PIXELS.iter().enumerate() {
        for (x, pixel) in row.bytes().enumerate() {
            if pixel == b'#' {
                svg.push_str(&format!(r#"<rect x="{x}" y="{y}" width="1" height="1"/>"#));
            }
        }
    }
    svg.push_str("</svg>");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Paper;

    #[test]
    fn logo_fits_both_receipt_widths() {
        assert!(PIXELS.iter().all(|row| row.len() == 24));
        for paper in [Paper::Mm58, Paper::Mm80] {
            for blocks in [false, true] {
                assert!(
                    text(paper.columns(), blocks)
                        .lines()
                        .all(|line| line.chars().count() <= paper.columns())
                );
            }
        }
    }
}
