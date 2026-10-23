//! Tray icons, drawn in code so no image files ship with the app.
//!
//! Each icon is a few shapes described by signed distance functions in a unit square
//! (y grows downwards), rendered with anti-aliased edges. Template icons are black
//! with alpha, so the OS can tint them for light and dark menu bars.

use crate::facade::TrayStatus;

/// Width and height in pixels: 22 pt at 2× for Retina menu bars.
pub const SIZE: u16 = 44;

/// Apple's system red, for the recording icon.
const RED: [u8; 3] = [0xFF, 0x3B, 0x30];
const BLACK: [u8; 3] = [0, 0, 0];

/// Half the stroke width, in unit coordinates.
const STROKE: f32 = 0.04;

/// An RGBA image for the tray.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Icon {
    /// `SIZE * SIZE` pixels, 4 bytes each, row by row.
    pub rgba: Vec<u8>,
    pub size: u16,
    /// Whether the OS should tint it to match the menu bar.
    pub template: bool,
}

/// The icon for `status`: a microphone when idle, a red filled microphone while
/// recording, three dots while busy, and a struck-through microphone on error.
#[must_use]
pub fn icon(status: TrayStatus) -> Icon {
    let template = status != TrayStatus::Recording;
    let color = if template { BLACK } else { RED };
    let rgba = (0..SIZE)
        .flat_map(|row| (0..SIZE).map(move |column| (column, row)))
        .flat_map(|(column, row)| {
            let point = (center(column), center(row));
            let [r, g, b] = color;
            [r, g, b, alpha(distance(status, point))]
        })
        .collect();
    Icon {
        rgba,
        size: SIZE,
        template,
    }
}

/// The centre of pixel `index`, in unit coordinates.
fn center(index: u16) -> f32 {
    (f32::from(index) + 0.5) / f32::from(SIZE)
}

/// Coverage of a pixel whose centre is `distance` (unit coordinates) from the edge.
fn alpha(distance: f32) -> u8 {
    let coverage = (0.5 - distance * f32::from(SIZE)).clamp(0.0, 1.0);
    // In range 0..=255 after the clamp, and `as` saturates anyway.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let byte = (coverage * 255.0).round() as u8;
    byte
}

type Point = (f32, f32);

fn distance(status: TrayStatus, point: Point) -> f32 {
    match status {
        TrayStatus::Idle => microphone(point, false),
        TrayStatus::Recording => microphone(point, true),
        TrayStatus::Transcribing | TrayStatus::Rewriting => [0.22, 0.5, 0.78]
            .into_iter()
            .map(|x| circle(point, (x, 0.5), 0.09))
            .fold(f32::INFINITY, f32::min),
        TrayStatus::Error => {
            microphone(point, false).min(stroke(segment(point, (0.16, 0.16), (0.84, 0.84))))
        }
    }
}

/// A capsule on a stand, outlined or filled.
fn microphone(point: Point, filled: bool) -> f32 {
    let capsule = segment(point, (0.5, 0.28), (0.5, 0.42)) - 0.14;
    let capsule = if filled { capsule } else { stroke(capsule) };
    [
        capsule,
        stroke(cradle(point)),
        stroke(segment(point, (0.5, 0.70), (0.5, 0.84))),
        stroke(segment(point, (0.36, 0.86), (0.64, 0.86))),
    ]
    .into_iter()
    .fold(f32::INFINITY, f32::min)
}

/// The U-shaped holder below the capsule: the lower half of a circle.
fn cradle(point: Point) -> f32 {
    let (center, radius) = ((0.5, 0.46), 0.24);
    if point.1 >= center.1 {
        circle(point, center, radius).abs()
    } else {
        length(sub(point, (center.0 - radius, center.1)))
            .min(length(sub(point, (center.0 + radius, center.1))))
    }
}

fn stroke(distance: f32) -> f32 {
    distance.abs() - STROKE
}

fn circle(point: Point, center: Point, radius: f32) -> f32 {
    length(sub(point, center)) - radius
}

/// Distance from `point` to the line segment from `a` to `b`.
fn segment(point: Point, a: Point, b: Point) -> f32 {
    let (pa, ba) = (sub(point, a), sub(b, a));
    let t = (dot(pa, ba) / dot(ba, ba)).clamp(0.0, 1.0);
    length(sub(pa, (ba.0 * t, ba.1 * t)))
}

fn sub(a: Point, b: Point) -> Point {
    (a.0 - b.0, a.1 - b.1)
}

fn dot(a: Point, b: Point) -> f32 {
    a.0 * b.0 + a.1 * b.1
}

fn length(a: Point) -> f32 {
    dot(a, a).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [TrayStatus; 5] = [
        TrayStatus::Idle,
        TrayStatus::Recording,
        TrayStatus::Transcribing,
        TrayStatus::Rewriting,
        TrayStatus::Error,
    ];

    /// The RGBA bytes of the pixel nearest to unit coordinates `(x, y)`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn pixel(icon: &Icon, (x, y): Point) -> [u8; 4] {
        let size = f32::from(icon.size);
        let (column, row) = ((x * size) as usize, (y * size) as usize);
        let start = (row * usize::from(icon.size) + column) * 4;
        icon.rgba[start..start + 4].try_into().unwrap()
    }

    #[test]
    fn icons_are_square_rgba_with_clear_corners() {
        ALL.iter().map(|status| icon(*status)).for_each(|icon| {
            assert_eq!(icon.rgba.len(), usize::from(SIZE) * usize::from(SIZE) * 4);
            [(0.0, 0.0), (0.99, 0.0), (0.0, 0.99)]
                .iter()
                .for_each(|corner| assert_eq!(pixel(&icon, *corner)[3], 0));
        });
    }

    #[test]
    fn only_the_recording_icon_is_red_and_not_a_template() {
        let recording = icon(TrayStatus::Recording);

        assert!(!recording.template);
        assert_eq!(pixel(&recording, (0.5, 0.35)), [0xFF, 0x3B, 0x30, 255]);
        ALL.iter()
            .filter(|status| **status != TrayStatus::Recording)
            .for_each(|status| assert!(icon(*status).template));
    }

    #[test]
    fn idle_microphone_is_an_outline() {
        let idle = icon(TrayStatus::Idle);

        assert_eq!(pixel(&idle, (0.5, 0.35))[3], 0, "hollow inside");
        assert_eq!(pixel(&idle, (0.36, 0.35))[3], 255, "capsule edge");
        assert_eq!(pixel(&idle, (0.5, 0.70))[3], 255, "cradle");
        assert_eq!(pixel(&idle, (0.26, 0.47))[3], 255, "cradle tip");
        assert_eq!(pixel(&idle, (0.5, 0.86))[3], 255, "base");
    }

    #[test]
    fn busy_shows_three_dots() {
        let busy = icon(TrayStatus::Transcribing);

        [0.22, 0.5, 0.78]
            .iter()
            .for_each(|x| assert_eq!(pixel(&busy, (*x, 0.5))[3], 255));
        assert_eq!(pixel(&busy, (0.36, 0.5))[3], 0);
        assert_eq!(busy, icon(TrayStatus::Rewriting));
    }

    #[test]
    fn error_strikes_through_the_microphone() {
        let error = icon(TrayStatus::Error);

        assert_eq!(pixel(&error, (0.2, 0.2))[3], 255);
        assert_eq!(pixel(&icon(TrayStatus::Idle), (0.2, 0.2))[3], 0);
    }

    #[test]
    fn edges_are_anti_aliased() {
        assert_eq!(alpha(-1.0), 255);
        assert_eq!(alpha(1.0), 0);
        assert_eq!(alpha(0.0), 128);
    }
}
