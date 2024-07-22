use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use derive_more::{Display, From};
use log::info;
use serde::Serialize;

use crate::types::ProofWrapper;
use crate::{fossil_mmr, types::Update};

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, Display, From)]
pub enum Error {
    #[from]
    MMRError(fossil_mmr::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        // How we want errors responses to be serialized
        #[derive(Serialize)]
        struct ErrorResponse {
            message: String,
        }

        let (status, message) = match self {
            Error::MMRError(err) => (StatusCode::EXPECTATION_FAILED, format!("{err:#?}")),
        };

        let body = Json(serde_json::json!({ 
            "error": message,
        }));

        (status, body).into_response()
    }
}

pub async fn get_mmr_latest() -> Result<Json<Update>> {
    info!("Received request for latest mmr");

    let res = fossil_mmr::get_mmr_stats().await?;
    Ok(Json(res))
}

pub async fn get_mmr_proof(Path(blocknumber): Path<i64>) -> Result<Json<ProofWrapper>> {
    info!("Received request for proof for block {blocknumber}");

    let res = fossil_mmr::get_proof(blocknumber).await?;
    Ok(Json(ProofWrapper { proof: res }))
}

// pub async fn verify_mmr_proof(Json(payload): Json<Proof>) -> fossil_mmr::Result<Json<bool>> {
//     let res = fossil_mmr::verify(blocknumber)
//         .await
//         .expect(format!("Error getting proof for {}", blocknumber).as_str());
//     Ok(Json(res))
// }
