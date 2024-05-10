use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use trustify_common::db::Transactional;
use trustify_entity::organization;

use crate::graph::{error::Error, Graph};

pub struct OrganizationInformation {
    pub cpe_key: Option<String>,
    pub website: Option<String>,
}

impl OrganizationInformation {
    pub fn has_data(&self) -> bool {
        self.cpe_key.is_some() || self.website.is_some()
    }
}

impl From<()> for OrganizationInformation {
    fn from(_: ()) -> Self {
        Self {
            cpe_key: None,
            website: None,
        }
    }
}

pub struct OrganizationContext<'g> {
    graph: &'g Graph,
    pub organization: organization::Model,
}

impl<'g> OrganizationContext<'g> {
    pub fn new(graph: &'g Graph, organization: organization::Model) -> Self {
        Self {
            graph,
            organization,
        }
    }
}

impl super::Graph {
    pub async fn get_organization_by_name<TX: AsRef<Transactional>>(
        &self,
        name: impl Into<String>,
        tx: TX,
    ) -> Result<Option<OrganizationContext>, Error> {
        Ok(organization::Entity::find()
            .filter(organization::Column::Name.eq(name.into()))
            .one(&self.connection(&tx))
            .await?
            .map(|organization| OrganizationContext::new(self, organization)))
    }

    pub async fn ingest_organization<TX: AsRef<Transactional>>(
        &self,
        name: impl Into<String>,
        information: impl Into<OrganizationInformation>,
        tx: TX,
    ) -> Result<OrganizationContext, Error> {
        let name = name.into();
        let information = information.into();

        if let Some(found) = self.get_organization_by_name(&name, &tx).await? {
            if information.has_data() {
                let mut entity = organization::ActiveModel::from(found.organization);
                entity.website = Set(information.website);
                entity.cpe_key = Set(information.cpe_key);
                let model = entity.update(&self.connection(&tx)).await?;
                Ok(OrganizationContext::new(found.graph, model))
            } else {
                Ok(found)
            }
        } else {
            let entity = organization::ActiveModel {
                id: Default::default(),
                name: Set(name),
                cpe_key: Set(information.cpe_key),
                website: Set(information.website),
            };

            Ok(OrganizationContext::new(
                self,
                entity.insert(&self.connection(&tx)).await?,
            ))
        }
    }
}
