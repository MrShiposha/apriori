use {
    super::r#type::{Distance, RelativeTime, Vector},
    std::ops::Range,
};

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

pub fn ranged_secant(
    valid_range: Range<RelativeTime>,
    eps: f32,
    f: impl Fn(RelativeTime) -> Distance,
) -> Option<RelativeTime> {
    let mut min = valid_range.start;
    let mut max = valid_range.end;

    let mut diff = max - min;
    let mut f_min = f(min);
    let mut f_max = f(max);
    let mut scale = diff / (f_max - f_min);

    while diff.abs() > eps {
        min = max - scale * f_max;
        max = min + scale * f_min;
        diff = max - min;
        f_min = f(min);
        f_max = f(max);

        scale = diff / (f_max - f_min);

        if !valid_range.contains(&max) {
            return None;
        }
    }

    if f_max.abs() <= eps {
        Some(max)
    } else {
        None
    }
}
