//! Exact collinearity of finite binary64 inputs using bounded integer sums.
//! This proves zero only; the caller still refuses uncertain nonzero areas.
const LIMBS: usize = 67;

fn parts(v: f64) -> Result<(bool, u64, i32), String> {
    if !v.is_finite() {
        return Err("nonfinite_exact_predicate".into());
    }
    let bits = v.to_bits();
    let exponent = ((bits >> 52) & 2047) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    Ok((
        bits >> 63 != 0,
        if exponent == 0 {
            fraction
        } else {
            fraction | (1 << 52)
        },
        if exponent == 0 {
            -1074
        } else {
            exponent - 1075
        },
    ))
}

fn add_word(sum: &mut [u64; LIMBS], mut index: usize, mut word: u64) -> Result<(), String> {
    while word != 0 {
        let cell = sum.get_mut(index).ok_or("exact_predicate_overflow")?;
        let (value, carry) = cell.overflowing_add(word);
        *cell = value;
        word = u64::from(carry);
        index += 1;
    }
    Ok(())
}

pub(crate) fn collinear(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> Result<bool, String> {
    let mut positive = [0u64; LIMBS];
    let mut negative = [0u64; LIMBS];
    // Expanded determinant cancels ax*ay algebraically, before any rounding.
    for (x, y, subtract) in [
        (b[0], c[1], false),
        (b[0], a[1], true),
        (a[0], c[1], true),
        (b[1], c[0], true),
        (b[1], a[0], false),
        (a[1], c[0], false),
    ] {
        let (sx, mx, ex) = parts(x)?;
        let (sy, my, ey) = parts(y)?;
        let product = u128::from(mx) * u128::from(my);
        let shift = (ex + ey + 2148) as usize;
        let sum = if sx ^ sy ^ subtract {
            &mut negative
        } else {
            &mut positive
        };
        for (offset, word) in [(0, product as u64), (1, (product >> 64) as u64)] {
            let index = shift / 64 + offset;
            let bits = shift % 64;
            add_word(sum, index, word << bits)?;
            if bits != 0 {
                add_word(sum, index + 1, word >> (64 - bits))?;
            }
        }
    }
    Ok(positive == negative)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_extremes_and_invalid_values() {
        assert!(!collinear([0., 0.], [134217729., 134217728.], [134217728., 134217727.]).unwrap());
        assert!(collinear([0., 0.], [f64::MAX, f64::MAX], [-f64::MAX, -f64::MAX]).unwrap());
        assert!(!collinear([0., 0.], [f64::from_bits(1), 0.], [0., f64::from_bits(1)]).unwrap());
        assert!(collinear([f64::NAN, 0.], [0., 0.], [0., 0.]).is_err());
    }
    #[test]
    fn integer_reference_comparisons() {
        let mut seed = 17u64;
        for _ in 0..10_000 {
            let mut coordinates = [0i128; 6];
            for value in &mut coordinates {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                *value = i128::from((seed >> 32) as i32);
            }
            let [ax, ay, bx, by, cx, cy] = coordinates;
            let expected = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax) == 0;
            assert_eq!(
                collinear(
                    [ax as f64, ay as f64],
                    [bx as f64, by as f64],
                    [cx as f64, cy as f64]
                )
                .unwrap(),
                expected
            );
            assert!(collinear(
                [ax as f64, ay as f64],
                [bx as f64, by as f64],
                [ax as f64, ay as f64]
            )
            .unwrap());
        }
    }
}
