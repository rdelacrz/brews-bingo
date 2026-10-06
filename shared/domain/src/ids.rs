use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, str::FromStr};
use uuid::Uuid;

mod error;
pub use error::IdError;

macro_rules! typed_id {
    ($($name:ident),+ $(,)?) => {$ (
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Uuid);
        impl TryFrom<Uuid> for $name {
            type Error = IdError;
            fn try_from(value: Uuid) -> Result<Self, IdError> {
                if value.get_version_num() == 7 && value.get_variant() == uuid::Variant::RFC4122 {
                    Ok(Self(value))
                } else { Err(IdError) }
            }
        }
        impl FromStr for $name {
            type Err = IdError;
            fn from_str(value: &str) -> Result<Self, IdError> {
                let parsed = Uuid::parse_str(value).map_err(|_| IdError)?;
                if value.len() != 36 || parsed.hyphenated().to_string() != value { return Err(IdError); }
                parsed.try_into()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { self.0.hyphenated().fmt(f) }
        }
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                text.parse().map_err(serde::de::Error::custom)
            }
        }
    )+};
}
typed_id!(
    AccountId,
    SessionId,
    LinkId,
    CommandId,
    GameId,
    PlayerId,
    ConnectionId,
    OperationId,
    AuditId
);

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, reason = "Tests fail fast.")]
mod tests {
    use super::*;
    #[test]
    fn player_id_preserves_canonical_uuid_v7_and_distinct_identity() {
        let text = "01890f3e-53b7-7d28-9b05-4f65092d5711";
        assert_eq!(text.parse::<PlayerId>().unwrap().to_string(), text);
        assert!(text.to_uppercase().parse::<PlayerId>().is_err());
        assert!(
            "01890f3e-53b7-4d28-9b05-4f65092d5711"
                .parse::<PlayerId>()
                .is_err()
        );
        assert_ne!(
            std::any::TypeId::of::<PlayerId>(),
            std::any::TypeId::of::<AccountId>()
        );
    }
    #[test]
    fn audit_identifiers_are_distinct_and_reject_noncanonical_values() {
        let text = "01890f3e-53b7-7d28-9b05-4f65092d5711";
        let audit: AuditId = text.parse().unwrap();
        assert_eq!(audit.to_string(), text);
        assert_ne!(
            std::any::TypeId::of::<AuditId>(),
            std::any::TypeId::of::<OperationId>()
        );
        assert!(text.to_uppercase().parse::<AuditId>().is_err());
        assert!(
            "01890f3e-53b7-4d28-9b05-4f65092d5711"
                .parse::<AuditId>()
                .is_err()
        );
    }

    #[test]
    fn ids_require_canonical_uuid_v7_and_preserve_type_identity() {
        let text = "01890f3e-53b7-7d28-9b05-4f65092d5711";
        let id: AccountId = text.parse().unwrap();
        assert_eq!(id.to_string(), text);
        assert!(
            "01890f3e-53b7-4d28-9b05-4f65092d5711"
                .parse::<AccountId>()
                .is_err()
        );
        assert!(text.to_uppercase().parse::<AccountId>().is_err());
        assert!(text.replace('-', "").parse::<AccountId>().is_err());
        assert!(AccountId::try_from(uuid::Uuid::nil()).is_err());
    }
}
