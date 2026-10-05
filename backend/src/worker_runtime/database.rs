//! Durable Object SQLite adapter.
use crate::db::{Database, Row, SqlValue, StorageError};
use crate::limits::SQL_QUERY_MAX_ROWS;
use js_sys::{Function, Reflect};
use wasm_bindgen::{JsCast, JsValue, closure::ScopedClosure};
use worker::{SqlStorageValue, Storage};

pub(super) struct OwnerDatabase {
    storage: Storage,
}
impl OwnerDatabase {
    pub(super) fn new(storage: Storage) -> Self {
        Self { storage }
    }
    fn bindings(params: &[SqlValue]) -> Result<Vec<SqlStorageValue>, StorageError> {
        params
            .iter()
            .map(|v| {
                Ok(match v {
                    SqlValue::Null => SqlStorageValue::Null,
                    SqlValue::Integer(n) => {
                        SqlStorageValue::try_from_i64(*n).map_err(|_| StorageError)?
                    }
                    SqlValue::Text(s) => SqlStorageValue::String(s.clone()),
                    SqlValue::Blob(b) => SqlStorageValue::Blob(b.clone()),
                })
            })
            .collect()
    }
}
impl Database for OwnerDatabase {
    fn query(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, StorageError> {
        let cursor = self
            .storage
            .sql()
            .exec(sql, Self::bindings(params)?)
            .map_err(|_| StorageError)?;
        let mut rows = Vec::new();
        for raw in cursor.raw() {
            if rows.len() >= SQL_QUERY_MAX_ROWS {
                return Err(StorageError);
            }
            let row = raw
                .map_err(|_| StorageError)?
                .into_iter()
                .map(|value| {
                    Ok(match value {
                        SqlStorageValue::Null => SqlValue::Null,
                        SqlStorageValue::Integer(n) => SqlValue::Integer(n),
                        SqlStorageValue::String(s) => SqlValue::Text(s),
                        SqlStorageValue::Blob(b) => SqlValue::Blob(b),
                        _ => return Err(StorageError),
                    })
                })
                .collect::<Result<Row, StorageError>>()?;
            rows.push(row);
        }
        Ok(rows)
    }
    fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<(), StorageError> {
        self.storage
            .sql()
            .exec(sql, Self::bindings(params)?)
            .map(|_| ())
            .map_err(|_| StorageError)
    }
    fn transaction<T>(
        &self,
        operation: impl FnOnce() -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let raw = self.storage.as_raw();
        let transaction = Reflect::get(raw.as_ref(), &JsValue::from_str("transactionSync"))
            .map_err(|_| StorageError)?
            .dyn_into::<Function>()
            .map_err(|_| StorageError)?;
        let mut operation = Some(operation);
        let mut result = None;
        let mut callback = || -> Result<JsValue, JsValue> {
            let operation = operation
                .take()
                .ok_or_else(|| JsValue::from_str("transaction callback already used"))?;
            match operation() {
                Ok(value) => {
                    result = Some(value);
                    Ok(JsValue::UNDEFINED)
                }
                Err(_) => Err(JsValue::from_str("storage transaction failed")),
            }
        };
        // Throw inside transactionSync to roll back; the callback cannot escape.
        let closure = ScopedClosure::borrow_mut_aborting(&mut callback);
        let completed = transaction.call1(raw.as_ref(), closure.as_ref());
        drop(closure);
        drop(callback);
        completed.map_err(|_| StorageError)?;
        result.ok_or(StorageError)
    }
}
