#[cfg(test)]
mod test;

use crate::advisory::service::AdvisoryService;
use crate::Error;
use actix_web::{get, post, web, HttpResponse, Responder};
use futures_util::TryStreamExt;
use std::str::FromStr;
use tokio_util::io::ReaderStream;
use trustify_common::db::query::Query;
use trustify_common::db::Database;
use trustify_common::id::Id;
use trustify_common::model::Paginated;
use trustify_module_ingestor::service::{Format, IngestorService};
use trustify_module_storage::service::StorageBackend;
use utoipa::{IntoParams, OpenApi};

pub fn configure(config: &mut web::ServiceConfig, db: Database) {
    let advisory_service = AdvisoryService::new(db);

    config
        .app_data(web::Data::new(advisory_service))
        .service(all)
        .service(get)
        .service(upload)
        .service(download);
}

#[derive(OpenApi)]
#[openapi(
    paths(all, get, upload, download),
    components(schemas(
        crate::advisory::model::AdvisoryDetails,
        crate::advisory::model::AdvisoryHead,
        crate::advisory::model::AdvisorySummary,
        crate::advisory::model::AdvisoryVulnerabilityHead,
        crate::advisory::model::AdvisoryVulnerabilitySummary,
        crate::advisory::model::PaginatedAdvisorySummary,
        trustify_common::advisory::AdvisoryVulnerabilityAssertions,
        trustify_common::advisory::Assertion,
        trustify_common::purl::Purl,
        trustify_common::id::Id,
    )),
    tags()
)]
pub struct ApiDoc;

#[utoipa::path(
    tag = "advisory",
    context_path = "/api",
    params(
        Query,
        Paginated,
    ),
    responses(
        (status = 200, description = "Matching vulnerabilities", body = PaginatedAdvisorySummary),
    ),
)]
#[get("/v1/advisory")]
pub async fn all(
    state: web::Data<AdvisoryService>,
    web::Query(search): web::Query<Query>,
    web::Query(paginated): web::Query<Paginated>,
) -> actix_web::Result<impl Responder> {
    Ok(HttpResponse::Ok().json(state.fetch_advisories(search, paginated, ()).await?))
}

#[utoipa::path(
    tag = "advisory",
    context_path = "/api",
    params(
        ("key" = string, Path, description = "Digest/hash of the document, prefixed by hash type, such as 'sha256:<hash>' or 'urn:uuid:<uuid>'"),
    ),
    responses(
        (status = 200, description = "Matching advisory", body = AdvisoryDetails),
        (status = 404, description = "Matching advisory not found"),
    ),
)]
#[get("/v1/advisory/{key}")]
pub async fn get(
    state: web::Data<AdvisoryService>,
    key: web::Path<String>,
) -> actix_web::Result<impl Responder> {
    let hash_key = Id::from_str(&key).map_err(Error::HashKey)?;
    let fetched = state.fetch_advisory(hash_key, ()).await?;

    if let Some(fetched) = fetched {
        Ok(HttpResponse::Ok().json(fetched))
    } else {
        Ok(HttpResponse::NotFound().finish())
    }
}

#[derive(
    IntoParams, Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize,
)]
struct UploadParams {
    /// Optional issuer if it cannot be determined from advisory contents.
    issuer: Option<String>,
}

#[utoipa::path(
    tag = "advisory",
    context_path = "/api",
    request_body = Vec<u8>,
    params(UploadParams),
    responses(
        (status = 201, description = "Upload a file"),
        (status = 400, description = "The file could not be parsed as an advisory"),
    )
)]
#[post("/v1/advisory")]
/// Upload a new advisory
pub async fn upload(
    service: web::Data<IngestorService>,
    web::Query(UploadParams { issuer }): web::Query<UploadParams>,
    bytes: web::Bytes,
) -> Result<impl Responder, Error> {
    let fmt = Format::from_bytes(&bytes)?;
    let payload = ReaderStream::new(&*bytes);
    let result = service
        .ingest(("source", "rest-api"), issuer, fmt, payload)
        .await?;
    Ok(HttpResponse::Created().json(result))
}

#[utoipa::path(
    tag = "advisory",
    context_path = "/api",
    params(
        ("key" = String, Path, description = "Digest/hash of the document, prefixed by hash type, such as 'sha256:<hash>'"),
    ),
    responses(
        (status = 200, description = "Download a an advisory", body = Vec<u8>),
        (status = 404, description = "The document could not be found"),
    )
)]
#[get("/v1/advisory/{key}/download")]
pub async fn download(
    ingestor: web::Data<IngestorService>,
    advisory: web::Data<AdvisoryService>,
    key: web::Path<String>,
) -> Result<impl Responder, Error> {
    // the user requested id
    let id = Id::from_str(&key).map_err(Error::HashKey)?;

    // look up document by id
    let Some(advisory) = advisory.fetch_advisory(id, ()).await? else {
        return Ok(HttpResponse::NotFound().finish());
    };

    let stream = ingestor
        .get_ref()
        .storage()
        .clone()
        .retrieve(advisory.head.hashes.try_into()?)
        .await
        .map_err(Error::Storage)?
        .map(|stream| stream.map_err(Error::Storage));

    Ok(match stream {
        Some(s) => HttpResponse::Ok().streaming(s),
        None => HttpResponse::NotFound().finish(),
    })
}
