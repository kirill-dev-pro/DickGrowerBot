use derive_more::{Display, Error};

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Ratio(f64);

#[derive(Debug, Display, Error)]
pub struct InvalidRatioValue(#[error(not(source))] f64);

impl Ratio {
    pub fn new(value: f64) -> Result<Self, InvalidRatioValue> {
        if (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(InvalidRatioValue(value))
        }
    }

    pub fn to_value(self) -> f64 {
        self.0
    }
}
