use crate::service::advisory;
use actix_web::http::StatusCode;
use actix_web::{post, web, HttpRequest, HttpResponse, Responder};
use csaf::Csaf;
use sha2::{Digest, Sha256};
use trustify_module_graph::endpoints::Error;
use trustify_module_graph::graph::Graph;
use walker_common::utils::hex::Hex;

#[utoipa::path(responses((status = 200, description = "Upload a file")))]
#[post("/advisories")]
pub async fn upload_advisory(
    graph: web::Data<Graph>,
    req: HttpRequest,
    payload: web::Payload,
) -> Result<impl Responder, Error> {
    // TODO: investigate how to parse files from a stream
    let payload_bytes = payload.to_bytes().await?;
    let sha256 = Hex(&Sha256::digest(&payload_bytes)).to_lower();

    let csaf = serde_json::from_slice::<Csaf>(&payload_bytes).map_err(|_e| Error::BadRequest {
        msg: "File could not be parsed".to_string(),
        status: StatusCode::BAD_REQUEST,
    })?;

    let advisory_id = advisory::csaf::ingest(&graph, csaf, &sha256, req.path()).await?;

    Ok(HttpResponse::Created().json(advisory_id))
}

#[cfg(test)]
mod tests {
    use super::super::configure;

    use actix_web::test::TestRequest;
    use actix_web::web::Data;
    use actix_web::{test, App};
    use std::fs;
    use std::path::PathBuf;
    use std::str::FromStr;
    use std::sync::Arc;
    use trustify_common::db::Database;
    use trustify_module_graph::graph::Graph;

    #[actix_web::test]
    async fn upload_advisory() -> Result<(), anyhow::Error> {
        let state = Arc::new(Graph::new(Database::for_test("upload_advisory").await?));

        let app = test::init_service(
            App::new()
                .app_data(Data::from(state.clone()))
                .configure(configure),
        )
        .await;

        let pwd = PathBuf::from_str(env!("CARGO_MANIFEST_DIR"))?;
        let test_data = pwd.join("../etc/test-data");

        let advisory = test_data.join("cve-2023-33201.json");

        let payload = fs::read_to_string(advisory).expect("File not found");
        let uri = "/advisories";
        let request = TestRequest::post()
            .uri(uri)
            .set_payload(payload)
            .to_request();

        let response = test::call_service(&app, request).await;

        assert!(response.status().is_success());

        Ok(())
    }
}
