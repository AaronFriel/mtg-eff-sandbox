use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Wrapper for a serializeable value. We could later memoize this, change the
/// serialized format to a string, etc. For now, and for a compact on-the-wire
/// representation in JSON, we use a JSON value.
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct EffectValue {
  pub(crate) serialized: serde_json::Value,
}

/// Wrap and unwrap effect values.
impl EffectValue {
  pub(crate) fn new<T>(value: &T) -> serde_json::Result<EffectValue>
  where
    T: Serialize + DeserializeOwned + 'static,
  {
    Ok(EffectValue {
      serialized: serde_json::to_value(value)?,
    })
  }

  pub fn get<T: DeserializeOwned>(&self) -> serde_json::Result<T> {
    serde_json::from_value(self.serialized.clone())
  }
}

///
#[derive(Serialize)]
pub(crate) struct EffectTree {
  pub(crate) result: EffectValue,
  pub(crate) children: Vec<EffectTree>,
}
