use super::{Draft, UiData};
use bevy::prelude::*;

#[derive(Clone, Copy)]
pub(super) enum Slider {
    Music,
    Sfx,
    Voice,
    Scroll,
    Gamma,
}
impl Slider {
    pub const ALL: [Self; 5] = [
        Self::Music,
        Self::Sfx,
        Self::Voice,
        Self::Scroll,
        Self::Gamma,
    ];
    pub fn area(self, data: &UiData) -> Rect {
        data.options.area(match self {
            Self::Music => "SliderMusicVolume",
            Self::Sfx => "SliderSFXVolume",
            Self::Voice => "SliderVoiceVolume",
            Self::Scroll => "SliderScrollSpeed",
            Self::Gamma => "SliderGamma",
        })
    }
    pub const fn bounds(self) -> [i32; 2] {
        match self {
            Self::Scroll => [10, 145],
            _ => [0, 100],
        }
    }
    pub const fn value(self, draft: &Draft) -> i32 {
        match self {
            Self::Music => draft.music,
            Self::Sfx => draft.sfx,
            Self::Voice => draft.voice,
            Self::Scroll => draft.scroll,
            Self::Gamma => draft.gamma,
        }
    }
    pub const fn set(self, draft: &mut Draft, value: i32) {
        match self {
            Self::Music => draft.music = value,
            Self::Sfx => draft.sfx = value,
            Self::Voice => draft.voice = value,
            Self::Scroll => draft.scroll = value,
            Self::Gamma => draft.gamma = value,
        }
    }
}
fn integer(value: f32) -> i32 {
    num_traits::ToPrimitive::to_i32(&value.trunc()).expect("native slider coordinate")
}
fn real(value: i32) -> f32 {
    num_traits::ToPrimitive::to_f32(&value).expect("native slider coordinate")
}
fn ticks(area: Rect, [min, max]: [i32; 2]) -> f32 {
    (area.width() - 13.0) / real(max - min)
}
pub(super) fn thumb(area: Rect, position: i32, bounds: [i32; 2]) -> Rect {
    let offset = real(integer(real(position - bounds[0]) * ticks(area, bounds)))
        .clamp(0.0, area.width() - 13.0);
    let top = area.min + Vec2::new(offset, 10.0);
    Rect::from_corners(top, top + Vec2::new(13.0, 16.0))
}
pub(super) fn drag(area: Rect, left: f32, mouse: f32, bounds: [i32; 2]) -> i32 {
    if mouse > area.max.x - 6.0 {
        return bounds[1];
    }
    if mouse < area.min.x + 6.0 {
        return bounds[0];
    }
    let centre = left.trunc() + 6.0;
    if centre < area.min.x + 6.0 {
        bounds[0]
    } else if centre >= area.max.x - 6.0 {
        bounds[1]
    } else {
        (integer((centre - area.min.x - 6.0) / ticks(area, bounds)) + bounds[0])
            .clamp(bounds[0], bounds[1])
    }
}
pub(super) fn page(area: Rect, position: i32, mouse: f32, bounds: [i32; 2]) -> i32 {
    let centre = thumb(area, position, bounds).min.x - area.min.x + 6.0;
    let clicked = mouse.trunc() - area.min.x;
    let page = (area.width() / 5.0).trunc();
    let mut moved = if clicked >= centre {
        (centre + page).min(clicked)
    } else {
        (centre - page).max(clicked)
    };
    // Preserve GadgetHorizontalSliderInput's mixed local/screen clamp.
    if moved > area.min.x + area.width() - 6.0 {
        moved = area.min.x + area.height() - 6.0;
    }
    moved = moved.max(6.0);
    drag(area, area.min.x + moved - 6.0, mouse, bounds)
}

pub(super) struct Boxes {
    area: Rect,
    width: f32,
    count: usize,
    pub selected: usize,
}
pub(super) fn boxes(
    area: Rect,
    scale: f32,
    image_width: f32,
    position: i32,
    [min, max]: [i32; 2],
) -> Boxes {
    let width = (image_width * scale).trunc();
    let percent = real(position - min) / real(max - min);
    let selected_x = area.min.x + real(integer(percent * area.width()));
    let mut start = area.min.x;
    let mut end = start + width;
    let mut count = 0;
    let mut selected = 0;
    loop {
        if end >= area.max.x {
            break;
        }
        if start <= selected_x && position != min {
            selected += 1;
        }
        start = end + 2.0;
        end = start + width;
        count += 1;
    }
    let covered = end - area.min.x - width;
    let blank = area.width() - covered;
    let mut area = area;
    area.min.x += (blank / 2.0).trunc();
    Boxes {
        area,
        width,
        count,
        selected,
    }
}
impl Boxes {
    pub fn box_rect(&self, index: usize) -> Option<Rect> {
        if index >= self.count {
            return None;
        }
        let offset = real(i32::try_from(index).expect("slider box count")) * (self.width + 2.0);
        let x = self.area.min.x + offset;
        Some(Rect::from_corners(
            Vec2::new(x, self.area.min.y),
            Vec2::new(x + self.width, self.area.min.y + self.width),
        ))
    }
    pub fn highlight(&self, index: usize) -> Option<Rect> {
        if index > self.count {
            return None;
        }
        let step = self.width + 2.0;
        let origin = self.area.min.x - (step / 2.0).trunc();
        let offset = real(i32::try_from(index).expect("slider box count")) * step;
        let x = origin + offset;
        let y = self.area.min.y + (self.width / 3.0).trunc();
        Some(Rect::from_corners(
            Vec2::new(x, y),
            Vec2::new(x + self.width + 2.0, y + self.width + 2.0),
        ))
    }
}

pub(super) fn scale(area: Rect, factor: Vec2) -> Rect {
    Rect::from_corners(area.min * factor, area.max * factor)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_horizontal_slider_keeps_thumb_truncation_page_size_and_drag_endpoints() {
        let area = Rect::new(100.0, 50.0, 308.0, 76.0);
        assert_eq!(
            thumb(area, 50, [0, 100]),
            Rect::new(197.0, 60.0, 210.0, 76.0)
        );
        assert_eq!(thumb(area, 250, [0, 100]), thumb(area, 100, [0, 100]));
        assert_eq!(drag(area, 197.0, 203.0, [0, 100]), 49);
        assert_eq!(drag(area, 197.0, 309.0, [0, 100]), 100);
        assert_eq!(drag(area, 197.0, 99.0, [0, 100]), 0);
        assert_eq!(page(area, 0, 180.0, [0, 100]), 21);
        let scroll = thumb(area, 0, [10, 145]);
        assert_eq!(scroll.min.x.to_bits(), area.min.x.to_bits());
    }
    #[test]
    fn source_image_boxes_keep_two_physical_pixel_gaps_and_hover_background_at_double_scale() {
        let layout = boxes(
            Rect::new(200.0, 100.0, 616.0, 152.0),
            2.0,
            20.0,
            50,
            [0, 100],
        );
        assert_eq!(layout.count, 9);
        assert_eq!(layout.selected, 5);
        assert_eq!(
            layout.box_rect(0),
            Some(Rect::new(219.0, 100.0, 259.0, 140.0))
        );
        assert_eq!(
            layout.box_rect(1).unwrap().min.x.to_bits(),
            261.0_f32.to_bits()
        );
        assert_eq!(
            layout.highlight(0),
            Some(Rect::new(198.0, 113.0, 240.0, 155.0))
        );
        assert!(layout.box_rect(9).is_none());
        assert!(layout.highlight(9).is_some());
        assert!(layout.highlight(10).is_none());
        assert_eq!(
            boxes(Rect::new(0.0, 0.0, 208.0, 26.0), 1.0, 20.0, 0, [0, 100]).selected,
            0
        );
    }
}
