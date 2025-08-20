use std::sync::Arc;

use axum::Extension;
use axum_jsonschema::Json;
use schemars::JsonSchema;
use serde::Serialize;

use crate::{jwt::JwtManager, types::AppError};
use serde_json::{Map, Value};

#[derive(Debug, Serialize, JsonSchema)]
pub struct KeysOutput {
    pub keys: Vec<Map<String, Value>>, // we can't use `josekit::jwk::Jwk` directly because it does not implement the JsonSchema trait
}

/// Serve a JWKS containing the active ES256 public key derived from AWS KMS.
///
/// For now, this returns a single JWK entry. In the future, when key rotation is
/// introduced, this endpoint will include multiple keys where each entry is
/// distinguished by a unique `kid`.
pub async fn jwks_wellknown(
    Extension(jwt_manager): Extension<Arc<JwtManager>>,
) -> Result<Json<KeysOutput>, AppError> {
    let jwk = jwt_manager.current_jwk().await?;

    Ok(Json(KeysOutput { keys: vec![jwk] }))
}
