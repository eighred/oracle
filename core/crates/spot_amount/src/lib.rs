use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

/// Nonnegative spot amounts with eight decimal places, represented as integer atoms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Amount(u64);

const UNIT: u64 = 100_000_000;

impl Amount {
    pub const ZERO: Self = Self(0);

    pub const ATOMS_PER_UNIT: u64 = UNIT;

    pub fn atoms(self) -> u64 {
        self.0
    }

    pub fn parse(text: &str) -> Option<Self> {
        let (whole, fraction) = match text.split_once('.') {
            Some((whole, fraction)) if !fraction.is_empty() => (whole, fraction),
            Some(_) => return None,
            None => (text, ""),
        };
        if whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || fraction.len() > 8
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let mut atoms = whole.parse::<u64>().ok()?.checked_mul(UNIT)?;
        if !fraction.is_empty() {
            let digits = fraction.parse::<u64>().ok()?;
            atoms = atoms.checked_add(digits * 10_u64.pow(8 - fraction.len() as u32))?;
        }
        Some(Self(atoms))
    }

    pub fn aligns(self, step: Self) -> bool {
        step.0 != 0 && self.0.is_multiple_of(step.0)
    }

    pub fn valuation(self, price: Self, cash: Self) -> Result<(String, String), String> {
        let value = u128::from(self.0) * u128::from(price.0);
        let nav = value
            .checked_add(u128::from(cash.0) * u128::from(UNIT))
            .ok_or("NAV exceeds the supported spot range")?;
        Ok((decimal(value, 16), decimal(nav, 16)))
    }
}

fn decimal(atoms: u128, scale: u32) -> String {
    let unit = 10_u128.pow(scale);
    let whole = atoms / unit;
    let fraction = atoms % unit;
    if fraction == 0 {
        return whole.to_string();
    }
    let value = format!("{whole}.{fraction:0width$}", width = scale as usize);
    value.trim_end_matches('0').to_owned()
}

impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(de)?;
        Self::parse(&raw).ok_or_else(|| {
            D::Error::custom("expected an unsigned decimal string with up to eight places")
        })
    }
}

impl Serialize for Amount {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&decimal(u128::from(self.0), 8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_is_exact_at_boundaries() {
        for invalid in [
            "",
            "1.",
            "-1",
            ".5",
            "0.000000001",
            "1e-8",
            "2.1.0",
            "184467440738",
        ] {
            assert!(Amount::parse(invalid).is_none(), "{invalid}");
        }
        let atom = Amount::parse("0.00000001").unwrap();
        assert_eq!(
            atom.valuation(atom, Amount(0)).unwrap().1,
            "0.0000000000000001"
        );
        assert_eq!(
            Amount::parse("184467440737.09551615"),
            Some(Amount(u64::MAX))
        );
    }
}
