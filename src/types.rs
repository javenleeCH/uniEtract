use crate::cli::InputFormat;

/// A set of possible `Value` variants, used for compile-time type upper-bound checks.
///
/// This is deliberately an *upper bound*: an intersection that is non-empty only says
/// the stage chain is not provably impossible. Runtime value-level checks stay in place.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeSet(u8);

impl TypeSet {
    const DATA_BIT: u8 = 1 << 0;
    const TEXT_BIT: u8 = 1 << 1;
    const NODE_BIT: u8 = 1 << 2;

    pub const DATA: Self = Self(Self::DATA_BIT);
    pub const TEXT: Self = Self(Self::TEXT_BIT);
    pub const NODE: Self = Self(Self::NODE_BIT);
    pub const ALL: Self = Self(Self::DATA_BIT | Self::TEXT_BIT | Self::NODE_BIT);

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub fn describe(self) -> String {
        let mut names = Vec::new();
        if self.0 & Self::DATA_BIT != 0 {
            names.push("Data");
        }
        if self.0 & Self::TEXT_BIT != 0 {
            names.push("Text");
        }
        if self.0 & Self::NODE_BIT != 0 {
            names.push("Node");
        }
        if names.is_empty() {
            "nothing".to_owned()
        } else {
            names.join("|")
        }
    }
}

/// The `Value` variant a given input format feeds into the first stage.
/// `Auto` is resolved at runtime, so it cannot be checked before reading input.
pub fn source_types(format: InputFormat) -> Option<TypeSet> {
    match format {
        InputFormat::Auto => None,
        InputFormat::Json
        | InputFormat::Jsonl
        | InputFormat::Yaml
        | InputFormat::Toml
        | InputFormat::Csv
        | InputFormat::Tsv => Some(TypeSet::DATA),
        InputFormat::Xml | InputFormat::Html => Some(TypeSet::NODE),
        InputFormat::Text => Some(TypeSet::TEXT),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_record_size: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_record_size: 8 * 1024 * 1024,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersection_is_symmetric_and_meaningful() {
        assert!(TypeSet::DATA.intersects(TypeSet::DATA.union(TypeSet::TEXT)));
        assert!(!TypeSet::NODE.intersects(TypeSet::DATA.union(TypeSet::TEXT)));
        assert!(TypeSet::ALL.intersects(TypeSet::NODE));
    }

    #[test]
    fn auto_cannot_be_checked_statically() {
        assert_eq!(source_types(InputFormat::Auto), None);
        assert_eq!(source_types(InputFormat::Xml), Some(TypeSet::NODE));
        assert_eq!(source_types(InputFormat::Csv), Some(TypeSet::DATA));
    }
}
