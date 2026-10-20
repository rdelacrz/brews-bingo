use zeroize::Zeroize;

pub(super) struct SecretBody(pub serde_json::Value);
impl Drop for SecretBody {
    fn drop(&mut self) {
        clear_value(&mut self.0);
    }
}
pub(crate) fn clear_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(string) => string.zeroize(),
        serde_json::Value::Array(values) => values.iter_mut().for_each(clear_value),
        serde_json::Value::Object(values) => values.values_mut().for_each(clear_value),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_json_values_are_zeroized_recursively() {
        let mut value = serde_json::json!({
            "password": "secret",
            "nested": ["another secret", {"answer": "private"}],
            "count": 3
        });

        clear_value(&mut value);

        assert_eq!(value["password"], "");
        assert_eq!(value["nested"][0], "");
        assert_eq!(value["nested"][1]["answer"], "");
        assert_eq!(value["count"], 3);
    }
}
