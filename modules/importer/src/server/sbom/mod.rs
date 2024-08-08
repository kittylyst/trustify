mod report;
pub mod storage;

use crate::{
    model::SbomImporter,
    server::{
        common::{filter::Filter, validation},
        context::RunContext,
        report::{ReportBuilder, ReportVisitor, ScannerError},
        sbom::report::SbomReportVisitor,
        RunOutput,
    },
};
use parking_lot::Mutex;
use sbom_walker::{
    retrieve::RetrievingVisitor,
    source::{HttpOptions, HttpSource},
    validation::ValidationVisitor,
    walker::Walker,
};
use std::{sync::Arc, time::SystemTime};
use tracing::instrument;
use trustify_module_ingestor::{graph::Graph, service::IngestorService};
use url::Url;
use walker_common::fetcher::{Fetcher, FetcherOptions};

impl super::ImportRunner {
    #[instrument(skip(self), ret)]
    pub async fn run_once_sbom(
        &self,
        context: impl RunContext,
        importer: SbomImporter,
        last_success: Option<SystemTime>,
    ) -> Result<RunOutput, ScannerError> {
        let report = Arc::new(Mutex::new(ReportBuilder::new()));

        let url = Url::parse(&importer.source).map_err(|err| ScannerError::Critical(err.into()))?;

        let keys = importer
            .keys
            .into_iter()
            .map(|key| key.into())
            .collect::<Vec<_>>();
        let source = HttpSource::new(
            url,
            Fetcher::new(FetcherOptions::new().retries(importer.fetch_retries.unwrap_or_default()))
                .await?,
            HttpOptions::new().since(last_success).keys(keys),
        );

        // storage (called by validator)

        let ingestor = IngestorService::new(Graph::new(self.db.clone()), self.storage.clone());
        let storage = storage::StorageVisitor {
            context,
            source: importer.source,
            labels: importer.common.labels,
            ingestor,
            report: report.clone(),
            max_size: importer.size_limit.map(|size| size.as_u64()),
        };

        // wrap storage with report

        let storage = SbomReportVisitor(ReportVisitor::new(report.clone(), storage));

        // validate (called by retriever)

        //  because we might still have GPG v3 signatures
        let options = validation::options(importer.v3_signatures)?;
        let validation = ValidationVisitor::new(storage).with_options(options);

        // retriever (called by filter)

        let visitor = RetrievingVisitor::new(source.clone(), validation);

        // filter

        let filter = Filter::from_config(visitor, importer.only_patterns)?;

        // walker

        // FIXME: track progress
        Walker::new(source)
            .walk(filter)
            .await
            // if the walker fails, we record the outcome as part of the report, but skip any
            // further processing, like storing the marker
            .map_err(|err| ScannerError::Normal {
                err: err.into(),
                output: RunOutput {
                    report: report.lock().clone().build(),
                    continuation: None,
                },
            })?;

        Ok(match Arc::try_unwrap(report) {
            Ok(report) => report.into_inner(),
            Err(report) => report.lock().clone(),
        }
        .build()
        .into())
    }
}
