//! FX lists share the client stream, but scorch choice uses its integer overload.
use crate::model_animation::Animations;

pub(in crate::particles) enum Request {
    Real(f32, f32),
    Scorch,
}

pub(in crate::particles) enum Value {
    Real(f32),
    Scorch(i32),
}

pub(in crate::particles) fn source(
    animations: &mut Animations,
    generation: u32,
) -> impl FnMut(Request) -> Value + '_ {
    move |request| match request {
        Request::Real(low, high) => {
            Value::Real(super::super::random(animations, generation)(low, high))
        }
        Request::Scorch => Value::Scorch(animations.effect_scorch(generation)),
    }
}

pub(in crate::particles) fn real(
    random: &mut impl FnMut(Request) -> Value,
    low: f32,
    high: f32,
) -> f32 {
    let Value::Real(value) = random(Request::Real(low, high)) else {
        unreachable!("real client random request")
    };
    value
}

#[cfg(test)]
pub(in crate::particles) fn real_only(
    mut random: impl FnMut(f32, f32) -> f32,
) -> impl FnMut(Request) -> Value {
    move |request| {
        let Request::Real(low, high) = request else {
            panic!("fixture contains no scorch random request")
        };
        Value::Real(random(low, high))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fx_real_and_scorch_requests_share_one_stream_and_restart_together() {
        for seed in [0, 7, u32::MAX] {
            let mut actual = Animations::effect_fixture(seed);
            let mut expected = Animations::effect_fixture(seed);
            {
                let mut random = source(&mut actual, 1);
                assert_eq!(real(&mut random, 4.0, 4.0).to_bits(), 4.0_f32.to_bits());
                let first = 5.0 * expected.effect_random(1);
                assert_eq!(
                    real(&mut random, 2.0, 7.0).to_bits(),
                    (2.0 + first).to_bits()
                );
                let Value::Scorch(choice) = random(Request::Scorch) else {
                    panic!("scorch returns an integer")
                };
                assert_eq!(choice, expected.effect_scorch(1));
                let last = 8.0 * expected.effect_random(1);
                assert_eq!(
                    real(&mut random, -3.0, 5.0).to_bits(),
                    (-3.0 + last).to_bits()
                );
            }
            assert_eq!(
                actual.effect_random(1).to_bits(),
                expected.effect_random(1).to_bits()
            );
            let mut restarted = source(&mut actual, 2);
            let Value::Scorch(choice) = restarted(Request::Scorch) else {
                panic!("scorch returns an integer")
            };
            assert_eq!(choice, expected.effect_scorch(2));
        }
    }
}
