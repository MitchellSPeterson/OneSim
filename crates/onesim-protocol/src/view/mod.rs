//! ViewNode schema (guest -> host).

pub mod animation;
pub mod color;
pub mod modifier;
pub mod node;
pub mod shape;
pub mod text;

pub use animation::*;
pub use color::*;
pub use modifier::*;
pub use node::*;
pub use shape::*;
pub use text::*;

/// A Fabric view whose single child is a SwiftUI or Compose subtree.
pub const EXPO_UI_HOST: &str = "ViewManagerAdapter_ExpoUI_HostView";
/// A React Native subtree placed inside a SwiftUI or Compose layout.
pub const EXPO_UI_RN_HOST: &str = "ViewManagerAdapter_ExpoUI_RNHostView";

/// Float that may be the string `"inf"` on the wire.
pub mod inf {
    use serde::{Deserialize, Deserializer, Serializer};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Num {
        F(f64),
        S(String),
    }

    pub fn serialize<S: Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(x) if x.is_infinite() && *x > 0.0 => s.serialize_str("inf"),
            Some(x) => s.serialize_f64(*x),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
        match Option::<Num>::deserialize(d)? {
            None => Ok(None),
            Some(Num::F(x)) => Ok(Some(x)),
            Some(Num::S(s)) if s == "inf" || s == "+inf" => Ok(Some(f64::INFINITY)),
            Some(Num::S(s)) if s == "-inf" => Ok(Some(f64::NEG_INFINITY)),
            Some(Num::S(s)) => Err(serde::de::Error::custom(format!("bad number string {s:?}"))),
        }
    }
}

pub(crate) fn is_false(b: &bool) -> bool {
    !*b
}

pub(crate) fn is_true(b: &bool) -> bool {
    *b
}

pub(crate) fn yes() -> bool {
    true
}

pub(crate) fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

pub(crate) fn one() -> f64 {
    1.0
}

pub(crate) fn ten() -> f64 {
    10.0
}

pub(crate) fn is_ten(v: &f64) -> bool {
    (*v - 10.0).abs() < 1e-9
}
