//! Radial sector geometry for ancestor fan charts.

/// One annular sector in an Ahnentafel fan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FanSector {
    pub number: u32,
    /// Generation number where the root is generation one.
    pub generation: usize,
    pub start_angle: f64,
    pub end_angle: f64,
    pub inner_radius: f64,
    pub outer_radius: f64,
}

/// Lays out ancestor sectors for a fan chart.
///
/// `span_degrees` is distributed evenly across each generation, with the
/// father occupying the first half of the parent's sector and the mother the
/// second half. Angles use SVG screen coordinates (0° right, 90° down) and
/// sweep clockwise from `start_angle`.
pub fn layout(
    generations: usize,
    span_degrees: f64,
    start_angle: f64,
    radius: f64,
) -> Vec<FanSector> {
    if !(2..=8).contains(&generations)
        || !radius.is_finite()
        || radius <= 0.0
        || !span_degrees.is_finite()
        || span_degrees <= 0.0
        || !start_angle.is_finite()
    {
        return Vec::new();
    }
    let Some(last_number) = 1u32.checked_shl(generations as u32) else {
        return Vec::new();
    };
    let ring = radius / generations as f64;
    let mut sectors = Vec::with_capacity(last_number.saturating_sub(2) as usize);
    for number in 2..last_number {
        let generation = (u32::BITS - number.leading_zeros()) as usize;
        let count = 1u32 << (generation - 1);
        let index = number - count;
        let step = span_degrees / count as f64;
        let start = start_angle + index as f64 * step;
        sectors.push(FanSector {
            number,
            generation,
            start_angle: start,
            end_angle: start + step,
            inner_radius: ring * (generation - 1) as f64,
            outer_radius: ring * generation as f64,
        });
    }
    sectors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_fan_splits_each_generation_evenly() {
        let sectors = layout(3, 180.0, 180.0, 300.0);
        assert_eq!(sectors.len(), 6, "numbers 2 through 7");
        let numbers: Vec<u32> = sectors.iter().map(|sector| sector.number).collect();
        assert_eq!(numbers, vec![2, 3, 4, 5, 6, 7]);

        assert_eq!(sectors[0].start_angle, 180.0);
        assert_eq!(sectors[0].end_angle, 270.0);
        assert_eq!(sectors[1].start_angle, 270.0);
        assert_eq!(sectors[1].end_angle, 360.0);
        assert_eq!(sectors[2].start_angle, 180.0);
        assert_eq!(sectors[2].end_angle, 225.0);
        assert_eq!(sectors[5].end_angle, 360.0);

        assert_eq!(sectors[0].generation, 2);
        assert_eq!(sectors[0].inner_radius, 100.0);
        assert_eq!(sectors[0].outer_radius, 200.0);
        assert_eq!(sectors[5].inner_radius, 200.0);
        assert_eq!(sectors[5].outer_radius, 300.0);
    }

    #[test]
    fn full_circle_covers_three_hundred_sixty_degrees() {
        let sectors = layout(4, 360.0, 0.0, 400.0);
        assert_eq!(sectors.len(), 14);
        let generation_four: Vec<&FanSector> = sectors
            .iter()
            .filter(|sector| sector.generation == 4)
            .collect();
        assert_eq!(generation_four.len(), 8);
        assert_eq!(generation_four[0].start_angle, 0.0);
        assert_eq!(generation_four[0].end_angle, 45.0);
        assert_eq!(generation_four[7].end_angle, 360.0);
    }

    #[test]
    fn start_angle_rotates_the_whole_fan() {
        let sectors = layout(2, 90.0, 45.0, 100.0);
        assert_eq!(sectors[0].start_angle, 45.0);
        assert_eq!(sectors[0].end_angle, 90.0);
        assert_eq!(sectors[1].start_angle, 90.0);
        assert_eq!(sectors[1].end_angle, 135.0);
    }

    #[test]
    fn invalid_inputs_produce_no_sectors() {
        assert!(layout(0, 180.0, 0.0, 100.0).is_empty());
        assert!(layout(1, 180.0, 0.0, 100.0).is_empty());
        assert!(layout(9, 180.0, 0.0, 100.0).is_empty());
        assert!(layout(4, 0.0, 0.0, 100.0).is_empty());
        assert!(layout(4, 180.0, 0.0, 0.0).is_empty());
        assert!(layout(4, f64::NAN, 0.0, 100.0).is_empty());
    }
}
