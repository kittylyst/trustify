use crate::organization::OrganizationHead;
use crate::Error;
use serde::{Deserialize, Serialize};
use trustify_common::db::ConnectionOrTransaction;
use trustify_common::paginated;
use trustify_entity::organization;
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Clone, Debug, ToSchema)]
pub struct OrganizationSummary {
    #[serde(flatten)]
    pub head: OrganizationHead,
}

paginated!(OrganizationSummary);

impl OrganizationSummary {
    pub async fn from_entity(
        organization: &organization::Model,
        tx: &ConnectionOrTransaction<'_>,
    ) -> Result<Self, Error> {
        Ok(OrganizationSummary {
            head: OrganizationHead::from_entity(organization, tx).await?,
        })
    }

    pub async fn from_entities(
        organizations: &[organization::Model],
        tx: &ConnectionOrTransaction<'_>,
    ) -> Result<Vec<Self>, Error> {
        let mut summaries = Vec::new();

        for org in organizations {
            summaries.push(OrganizationSummary {
                head: OrganizationHead::from_entity(org, tx).await?,
            });
        }

        Ok(summaries)
    }
}
