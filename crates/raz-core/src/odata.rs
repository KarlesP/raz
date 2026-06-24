//! Tiny helper for the OData `$filter` query parameter used by ARM and Microsoft Graph.

/// URL-encoded `$filter=<field> eq '<value>'` (space → `%20`, quote → `%27`).
pub(crate) fn odata_eq(field: &str, value: &str) -> String {
    format!("$filter={field}%20eq%20%27{value}%27")
}
