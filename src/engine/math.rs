use {
    crate::r#type::{Distance, RelativeTime, Vector},
    std::ops::Range,
};

const INV_PHI: f32 = 0.618033;
const INV_PHI2: f32 = 0.381966;

pub fn hermite_interpolation(
    location_0: &Vector,
    velocity_0: &Vector,
    time_0: RelativeTime,
    location_1: &Vector,
    velocity_1: &Vector,
    time_1: RelativeTime,
    interest_time: RelativeTime,
) -> Vector {
    let step = time_1 - time_0;
    let t = (interest_time - time_0) / step;

    let t2 = t * t;
    let t3 = t2 * t;

    let p1_coeff = -2.0 * t3 + 3.0 * t2;
    let p0_coeff = -p1_coeff + 1.0;

    let p0 = location_0.scale(p0_coeff);
    let m0 = velocity_0.scale((t3 - 2.0 * t2 + t) * step);

    let p1 = location_1.scale(p1_coeff);
    let m1 = velocity_1.scale((t3 - t2) * step);

    p0 + m0 + p1 + m1
}

pub fn golden_section_search(
    valid_range: Range<RelativeTime>,
    t_eps: f32,
    f: &impl Fn(RelativeTime) -> Distance
) -> RelativeTime {
    let Range {
        mut start,
        mut end
    } = valid_range;

    debug_assert!(start < end);

    let mut diff = end - start;
    if diff.abs() <= t_eps {
        return (start + end) / 2.0;
    }

    let mut x1 = start + INV_PHI2 * diff;
    let mut x2 = start + INV_PHI * diff;

    let mut f1 = f(x1);
    let mut f2 = f(x2);

    while diff.abs() > t_eps {
        if f1 < f2 {
            end = x2;
            x2 = x1;
            f2 = f1;

            diff = end - start;
            x1 = start + INV_PHI2 * diff;
            f1 = f(x1);
        } else {
            start = x1;
            x1 = x2;
            f1 = f2;

            diff = end - start;
            x2 = start + INV_PHI * diff;
            f2 = f(x2);
        }
    }

    debug_assert!(start <= end, "start: {}, end: {}", start, end);

    (start + end) / 2.0
}

pub fn bisection(
    valid_range: &Range<RelativeTime>,
    t_eps: f32,
    f_eps: f32,
    f: &impl Fn(RelativeTime) -> Distance,
) -> Option<RelativeTime> {
    let Range {
        mut start,
        mut end
    } = valid_range;

    debug_assert!(start < end);

    if f(start)*f(end) > 0.0 {
        return None;
    }

    let mut diff = end - start;
    while diff.abs() >= t_eps {
        let mid = (start + end) / 2.0;
        let f_mid = f(mid);

        if f_mid.abs() <= f_eps {
            debug_assert!(start < end);
            return Some(mid);
        } else if f_mid * f(start) < 0.0 {
            end = mid;
        } else {
            start = mid;
        }

        diff = end - start;
    }

    None
}

pub fn find_root(
    mut valid_range: Range<RelativeTime>,
    t_eps: f32,
    f_eps: f32,
    f: impl Fn(RelativeTime) -> Distance,
) -> Option<RelativeTime> {
    let min_distance = golden_section_search(
        valid_range.clone(),
        t_eps,
        &f
    );

    valid_range.end = min_distance;

    bisection(&valid_range, t_eps, f_eps, &f)
}
