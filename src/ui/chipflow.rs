//! Where the chips of a group go: they fill a line, wrap onto the next one when one more does
//! not fit, and each line stretches to the full width, like a flex row that wraps.

use slint::{Model, ModelRc, SharedString, VecModel};

use super::ChipPos;

/// How wide each character from U+0020 to U+017F is, in thousandths of the font size: the
/// advances of OPPO Sans 4.0 EN at weight 500, the phone's system font that the UI draws with
/// on a OnePlus. Other fonts are near enough: a chip is only as wide as this says, and text that
/// is a little wider is cut short with an ellipsis.
const ADVANCE: [u16; 352] = [
    228, 275, 349, 609, 644, 933, 717, 198, 389, 389, 519, 645, 269, 389, 270, 409, 639, 407, 574,
    583, 628, 606, 619, 564, 610, 618, 270, 270, 645, 645, 645, 506, 931, 694, 634, 711, 720, 590,
    578, 732, 705, 234, 547, 639, 560, 916, 710, 746, 634, 746, 649, 604, 613, 713, 682, 1023, 669,
    668, 637, 389, 409, 389, 645, 508, 600, 539, 588, 537, 589, 551, 342, 586, 569, 223, 227, 521,
    222, 872, 569, 577, 589, 590, 382, 503, 346, 564, 530, 804, 501, 529, 484, 389, 259, 389, 645,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 228, 275, 644, 644, 644, 644, 259, 644, 600, 825, 382, 635, 645, 389, 518, 600, 359, 645,
    392, 391, 600, 606, 578, 500, 600, 317, 401, 635, 879, 921, 927, 506, 695, 695, 695, 695, 695,
    695, 1032, 711, 590, 590, 590, 590, 234, 234, 234, 234, 735, 710, 746, 746, 746, 746, 746, 645,
    746, 713, 713, 713, 713, 668, 634, 576, 539, 539, 539, 539, 539, 539, 892, 535, 547, 547, 547,
    547, 223, 223, 223, 223, 552, 569, 573, 573, 573, 573, 573, 645, 573, 564, 564, 564, 564, 529,
    587, 529, 695, 539, 695, 539, 695, 539, 711, 535, 711, 535, 711, 535, 711, 535, 720, 693, 735,
    609, 590, 547, 590, 547, 590, 547, 590, 547, 590, 547, 732, 583, 732, 583, 732, 583, 732, 583,
    705, 569, 804, 587, 234, 223, 234, 223, 234, 223, 234, 223, 234, 223, 781, 459, 547, 227, 639,
    522, 517, 560, 222, 560, 222, 560, 324, 560, 342, 625, 249, 710, 569, 710, 569, 710, 569, 582,
    715, 569, 746, 573, 746, 573, 746, 573, 1108, 950, 649, 382, 649, 382, 649, 382, 605, 503, 605,
    503, 605, 503, 605, 503, 614, 346, 614, 346, 614, 346, 713, 564, 713, 564, 713, 564, 713, 564,
    713, 564, 713, 564, 1023, 804, 668, 529, 668, 637, 484, 637, 484, 637, 484, 305,
];

/// A chip's natural width: the text, as wide as its characters say, plus its padding, and no
/// less than `min`.
fn basis(text: &str, pad: f32, min: f32, font_size: f32) -> f32 {
    let thousandths: u32 = text
        .chars()
        .map(|c| match c {
            '\u{2212}' => 645,
            c => (c as usize)
                .checked_sub(0x20)
                .and_then(|i| ADVANCE.get(i))
                .map_or(560, |w| u32::from(*w).max(1)),
        })
        .sum();
    (thousandths as f32 * font_size / 1000.0 + pad).max(min)
}

/// Line, x and width of each chip in a row `width` wide, `gap` apart. The room left on a line
/// is shared out equally, as `flex: 1 1 auto` does. A chip wider than a line has one to itself.
pub(super) fn place(
    labels: &[String],
    width: f32,
    gap: f32,
    pad: f32,
    min: f32,
    font_size: f32,
) -> Vec<(i32, f32, f32)> {
    let bases: Vec<f32> = labels
        .iter()
        .map(|text| basis(text, pad, min, font_size).min(width.max(min)))
        .collect();
    let mut out = Vec::with_capacity(bases.len());
    let mut start = 0;
    let mut line = 0;
    while start < bases.len() {
        let mut end = start + 1;
        let mut used = bases[start];
        while end < bases.len() && used + gap + bases[end] <= width {
            used += gap + bases[end];
            end += 1;
        }
        let share = (width - used).max(0.0) / (end - start) as f32;
        let mut x = 0.0;
        for base in &bases[start..end] {
            out.push((line, x, base + share));
            x += base + share + gap;
        }
        line += 1;
        start = end;
    }
    out
}

pub(super) fn positions(
    labels: ModelRc<SharedString>,
    width: f32,
    gap: f32,
    pad: f32,
    min: f32,
    font_size: f32,
) -> ModelRc<ChipPos> {
    let labels: Vec<String> = labels.iter().map(|text| text.to_string()).collect();
    ModelRc::new(VecModel::from(
        place(&labels, width, gap, pad, min, font_size)
            .into_iter()
            .map(|(line, x, w)| ChipPos { line, x, w })
            .collect::<Vec<_>>(),
    ))
}

#[cfg(test)]
mod tests {
    use super::place;

    fn labels(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn chips_that_fit_share_one_line_and_fill_it() {
        let got = place(&labels(&["5 min", "10 min"]), 300.0, 8.0, 12.0, 44.0, 14.0);
        assert_eq!(got.iter().map(|c| c.0).collect::<Vec<_>>(), vec![0, 0]);
        let end = got[1].1 + got[1].2;
        assert!((end - 300.0).abs() < 0.01, "ends at {end}");
        assert!((got[1].1 - (got[0].2 + 8.0)).abs() < 0.01);
    }

    #[test]
    fn one_more_chip_than_fits_goes_to_the_next_line() {
        let got = place(
            &labels(&["1 min", "5 min", "10 min", "30 min", "Forever"]),
            200.0,
            8.0,
            12.0,
            44.0,
            14.0,
        );
        let lines: Vec<i32> = got.iter().map(|c| c.0).collect();
        assert_eq!(lines, vec![0, 0, 0, 1, 1]);
        assert!(got[3].1.abs() < 0.01);
    }

    #[test]
    fn a_chip_wider_than_a_line_is_cut_to_it() {
        let got = place(
            &labels(&["a very long label indeed"]),
            100.0,
            8.0,
            12.0,
            44.0,
            14.0,
        );
        assert_eq!(got.len(), 1);
        assert!((got[0].2 - 100.0).abs() < 0.01);
    }

    #[test]
    fn no_chips_make_no_lines() {
        assert!(place(&[], 100.0, 8.0, 12.0, 44.0, 14.0).is_empty());
    }
}
